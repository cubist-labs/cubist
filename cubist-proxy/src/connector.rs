//! connector: an Pair <-> Pair connector

use futures::channel::mpsc::{self, Receiver, Sender};
use futures::{Sink, SinkExt, Stream, StreamExt};
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{FatalErr, Pair};

/// Connect two Pairs with matching types
pub async fn connect<S, R, E>(
    left: impl Pair<S, R, E>,
    right: impl Pair<R, S, E>,
) -> Result<(), FatalErr> {
    let (to_l, from_l) = left.split();
    let (to_r, from_r) = right.split();

    let l2r = from_l.map(Ok).forward(to_r);
    let r2l = from_r.map(Ok).forward(to_l);

    // connector dies if either connection finishes
    tokio::select! {
        res = l2r => res,
        res = r2l => res,
    }
    .map_err(Into::into)
}

/// Provides an entangled pair of `Connector`s so that a write to the left
/// side appears as a read on the right side, and vice-versa.
pub fn passthru<S, R>() -> (MpscPair<S, R>, MpscPair<R, S>) {
    // Setting queue depth to 1 lets us call prod.send()/cons.send()
    // before (rather than concurrently with) cons.next()/prod.next().
    // With queue depth 0, sequential send-next causes deadlock in
    // (most?) sink-combinator pipelines that use `with(...)`
    // (e.g., see the tests in the `transformer::convert` module).
    let (prod_w, cons_r) = mpsc::channel::<S>(1);
    let (cons_w, prod_r) = mpsc::channel::<R>(1);

    let prod = ConcretePair::from_parts(prod_w, prod_r);
    let cons = ConcretePair::from_parts(cons_w, cons_r);

    (prod, cons)
}

/// Provides an entangled pair of `Pair`s such that a write to the left
/// side appears as a read on the right side, and vice-versa.
///
/// Use this function when you need two endpoints that both implement
/// `Pair`. Because of the types of `Pair` and `Connector`, it is not
/// possible for the endpoints created by `passthru` to do so.
pub fn passthru_pair<S, R, E>() -> (impl Pair<S, R, E> + Unpin, impl Pair<R, S, E> + Unpin)
where
    S: Send,
    R: Send,
    E: Send,
{
    let (a, b) = passthru::<Result<S, E>, Result<R, E>>();
    (a.sink_err_into(), b.sink_err_into())
}

/// A Pair built from two send-receive pairs
#[pin_project]
pub struct ConcretePair<SS, RS> {
    /// The sending side, which implements Sink
    #[pin]
    sender: SS,

    /// The receiving side, which implements Stream
    #[pin]
    receiver: RS,
}

/// A ConcretePair made of an underlying mpsc sender and receiver
pub type MpscPair<S, R> = ConcretePair<Sender<S>, Receiver<R>>;

impl<S> ConcretePair<Sender<S>, Receiver<S>> {
    /// Create a new Connector that sends and receives the same type to itself
    pub fn pipe() -> Self {
        // we set queue depth to 1 so that you can call pipe.send() before
        // (rather than concurrently with) pipe.next(). With queue depth 0,
        // pipe.send() blocks until someone calls pipe.next(), so calling
        // them sequentially causes a deadlock.
        let (snd, rcv) = mpsc::channel::<S>(1);
        Self::from_parts(snd, rcv)
    }
}

impl<SS, RS> ConcretePair<SS, RS> {
    /// Create a new Connector from a Sender and Receiver
    pub fn from_parts<S>(sender: SS, receiver: RS) -> Self
    where
        SS: Sink<S>,
        RS: Stream,
    {
        Self { sender, receiver }
    }

    /// Extract the underlying Sender and Receiver from this Connector
    pub fn into_parts(self) -> (SS, RS) {
        (self.sender, self.receiver)
    }
}

impl<SS, RS> Stream for ConcretePair<SS, RS>
where
    RS: Stream,
{
    type Item = RS::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx)
    }
}

impl<S, SS, RS> Sink<S> for ConcretePair<SS, RS>
where
    SS: Sink<S>,
{
    type Error = SS::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: S) -> Result<(), Self::Error> {
        self.project().sender.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::JsonRpcErr;

    use futures::StreamExt;
    use rstest::rstest;
    use serde_json::{json, Value};

    #[rstest]
    #[case::drop_s1(0)]
    #[case::drop_r2(1)]
    #[tokio::test]
    async fn test_connect(#[case] which_to_drop: usize) {
        let (mut s1, r1) = passthru_pair::<Value, (), JsonRpcErr>();
        let (s2, mut r2) = passthru_pair::<Value, (), JsonRpcErr>();

        let handle = tokio::spawn(async move { connect(r1, s2).await });

        r2.send(Ok(())).await.unwrap();
        assert_eq!(s1.next().await.transpose().unwrap(), Some(()));

        s1.send(Ok(json!("asdf"))).await.unwrap();
        assert_eq!(
            r2.next().await.transpose().unwrap(),
            Some(Value::String("asdf".to_owned()))
        );

        // make sure handle dies if either end disconnects
        if which_to_drop == 0 {
            drop(s1);
        } else {
            drop(r2);
        }
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_passthru() {
        let (mut left, right) = passthru::<Value, Value>();

        let (r_snd, r_rcv) = right.split();
        tokio::spawn(async move { r_rcv.map(Ok).forward(r_snd).await });

        left.send(json!("qwer")).await.unwrap();
        assert_eq!(left.next().await, Some(Value::String("qwer".to_owned())));
    }

    #[tokio::test]
    async fn test_pipe() {
        let p = ConcretePair::pipe();
        let (mut p_snd, p_rcv) = p.into_parts();
        let mut p_rcv = p_rcv.map(Value::String);

        // send first, then receive; this tests our ability to use pipe sequentially
        p_snd.send("zxcv".to_owned()).await.unwrap();
        assert_eq!(p_rcv.next().await, Some(Value::String("zxcv".to_owned())));
    }
}
