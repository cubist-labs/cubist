// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! Switch transformer: Given a pair and a predicate, return a True-pair and a False-pair

use futures::stream::select;
use futures::{Sink, SinkExt, Stream, StreamExt};

use crate::connector::passthru_pair;
use crate::{FatalErr, Pair};

/// Given a Pair and a predicate, return two pairs, one for True and one for False
pub fn switch<S, R, E>(
    pair: impl Pair<S, R, E> + 'static,
    pred: impl Fn(&Result<R, E>) -> bool + Send + 'static,
) -> (impl Pair<S, R, E>, impl Pair<S, R, E>)
where
    S: Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
{
    let (ret_true, p_true) = passthru_pair();
    let (ret_false, p_false) = passthru_pair();

    tokio::spawn(async move {
        let (true_snd, true_rcv) = p_true.split();
        let (false_snd, false_rcv) = p_false.split();
        let (pair_snd, pair_rcv) = pair.split();

        let send_f = select(true_rcv, false_rcv).map(Ok).forward(pair_snd);
        let rcv_f = switch_stream(pair_rcv, pred, true_snd, false_snd);

        tokio::select! {
            res = send_f => {
                tracing::debug!("switch send_f exited with {res:?}");
            }
            res = rcv_f => {
                tracing::debug!("switch rcv_f exited with {res:?}");
            }
        }

        tracing::debug!("switch thread exiting");
    });

    (ret_true, ret_false)
}

/// Route values from an input stream to one of two output sinks according to a predicate
pub async fn switch_stream<T, E1, E2>(
    mut strm: impl Stream<Item = T> + Unpin,
    pred: impl Fn(&T) -> bool,
    mut send_t: impl Sink<T, Error = E1> + Unpin,
    mut send_f: impl Sink<T, Error = E2> + Unpin,
) -> Result<(), FatalErr>
where
    E1: Into<FatalErr>,
    E2: Into<FatalErr>,
{
    while let Some(v) = strm.next().await {
        let res = if pred(&v) {
            send_t.send(v).await.map_err(Into::into)
        } else {
            send_f.send(v).await.map_err(Into::into)
        };

        if res.is_err() {
            tracing::debug!("switch_stream send err: {res:?}");
            return res;
        }
    }

    Ok(())
}

/// Route values from an input stream to one of two output sinks according to
/// a function that converts the value to either the type of the `send_ok` stream
/// or the type of the `send_err` stream.
pub async fn switch_convert_stream<Ti, To1, To2, E1, E2>(
    mut strm: impl Stream<Item = Ti> + Unpin,
    conv: impl Fn(Ti) -> Result<To1, To2>,
    mut send_ok: impl Sink<To1, Error = E1> + Unpin,
    mut send_err: impl Sink<To2, Error = E2> + Unpin,
) -> Result<(), FatalErr>
where
    E1: Into<FatalErr>,
    E2: Into<FatalErr>,
{
    while let Some(v) = strm.next().await {
        let res = match conv(v) {
            Ok(to1) => send_ok.send(to1).await.map_err(Into::into),
            Err(to2) => send_err.send(to2).await.map_err(Into::into),
        };

        if res.is_err() {
            tracing::debug!("switch_convert_stream err: {res:?}");
            return res;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connector::passthru_pair;

    use futures::{channel::mpsc, FutureExt};
    use rstest::rstest;

    #[tokio::test]
    async fn test_switch() {
        let (mut snd, recv) = passthru_pair::<usize, usize, ()>();
        let (mut even, mut odd) = switch(recv, |x| {
            x.as_ref().ok().map(|x| x % 2 == 0).unwrap_or(false)
        });

        // even value goes to even
        snd.send(Ok(0)).await.unwrap();
        assert_eq!(even.next().await.transpose().unwrap(), Some(0));
        assert!(odd.next().now_or_never().is_none());

        // odd value goes to odd
        snd.send(Ok(1)).await.unwrap();
        assert_eq!(odd.next().await.transpose().unwrap(), Some(1));
        assert!(even.next().now_or_never().is_none());

        // err values also go to odd
        snd.send(Err(())).await.unwrap();
        assert_eq!(odd.next().await, Some(Err(())));
        assert!(even.next().now_or_never().is_none());

        // writing to odd
        odd.send(Ok(2)).await.unwrap();
        assert_eq!(snd.next().await.transpose().unwrap(), Some(2));
        assert!(even.next().now_or_never().is_none());

        // writing to even
        even.send(Ok(3)).await.unwrap();
        assert_eq!(snd.next().await.transpose().unwrap(), Some(3));
        assert!(odd.next().now_or_never().is_none());

        // writing to both even and odd before reading from snd
        even.send(Ok(4)).await.unwrap();
        odd.send(Ok(5)).await.unwrap();
        // no ordering guarantee!
        if let Some(x) = snd.next().await.transpose().unwrap() {
            assert!(x == 4 || x == 5);
            assert_eq!(snd.next().await.transpose().unwrap(), Some(9 - x));
        } else {
            unreachable!()
        }
        assert!(snd.next().now_or_never().is_none());
        assert!(even.next().now_or_never().is_none());
        assert!(odd.next().now_or_never().is_none());
    }

    #[rstest]
    #[case::drop_snd(0)]
    #[case::send_snd(1)]
    #[tokio::test]
    async fn test_switch_convert(#[case] send_kill: usize) {
        let (mut snd, r_u16) = mpsc::channel::<u16>(0);
        let (s_u32, mut even) = mpsc::channel::<u32>(0);
        let (s_u64, mut odd) = mpsc::channel::<u64>(0);

        let mut handle = tokio::spawn(async move {
            switch_convert_stream(
                r_u16,
                |x| {
                    if x % 2 == 0 {
                        Ok(x as u32)
                    } else {
                        Err(x as u64)
                    }
                },
                s_u32,
                s_u64,
            )
            .await
        });

        // odd values go to odd
        snd.send(0).await.unwrap();
        assert_eq!(even.next().await, Some(0));
        assert!(odd.next().now_or_never().is_none());

        // even values go to even
        snd.send(1).await.unwrap();
        assert_eq!(odd.next().await, Some(1));
        assert!(even.next().now_or_never().is_none());

        // closing receivers does not kill the thread
        drop(even);
        assert!(Box::pin(&mut handle).now_or_never().is_none());
        drop(odd);
        assert!(Box::pin(&mut handle).now_or_never().is_none());

        if send_kill == 0 {
            // closing sender kills the thread gracefully
            drop(snd);
            assert!(matches!(handle.await, Ok(Ok(()))));
        } else {
            // writing after receiver is gone kills the thread with an error
            snd.send(2).await.unwrap();
            assert!(matches!(handle.await, Ok(Err(_))));
        }
    }
}
