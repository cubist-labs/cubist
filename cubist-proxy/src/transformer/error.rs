//! Error-routing transformers

use futures::{channel::mpsc, stream::select, StreamExt, TryStreamExt};

use super::switch::switch_convert_stream;
use crate::{connector::passthru_pair, Pair};

/// Given a Pair, reroute all Err() values from its Stream into its Sink.
/// Returns a new Pair whose Stream outputs all Ok() values and whose Sink
/// connects to the input Pair's sink.
///
/// Convert the Error type of a Pair, returning a new Pair.
///
/// The results of this combinator are:
///
/// (1) Errors from the original Pair's Stream are redirected to its Sink,
///
/// ```text
/// //                  ||
/// // orig_stream -->      ----+----(ok)-----------> new_stream
/// //                  ||      |
/// // (Error = Ei)     ||    (err)                        (Error = Eo)
/// //                  ||      v
/// // orig_sink   <--      <---+--(map_err o into)-- new_sink
/// //                  ||
/// ```
///
/// (2) Errors into the new Pair's Sink are converted (via Into) to the
///     old pair's error type.
///
pub fn errors_to_sink<S, R, Ei, Eo>(pair: impl Pair<S, R, Ei> + 'static) -> impl Pair<S, R, Eo>
where
    R: Send + 'static,
    S: Send + 'static,
    Ei: Send + 'static,
    Eo: Into<Ei> + Send + 'static,
{
    let (theirs, ours) = passthru_pair::<S, R, Eo>();
    let (p_sender, p_receiver) = pair.split();
    let (o_sender, o_receiver) = ours.split();
    let (e_sender, e_receiver) = mpsc::channel::<Result<_, Ei>>(0);

    // convert errors sent to the new Pair's sink into Ei
    let o_receiver = o_receiver.map_err(Into::into);

    // select(o_receiver, e_receiver).forward(p_sender)
    // p_receiver -> |m| if m.is_err() { e_sender.send(m) } else { o_sender.send(m) }
    tokio::spawn(async move {
        let send_f = select(e_receiver, o_receiver).map(Ok).forward(p_sender);
        let rcv_f =
            switch_convert_stream(p_receiver, |x| x.map(Ok).map_err(Err), o_sender, e_sender);

        // no need for tokio::try_join! because we ignore the error here
        tokio::select! {
            res = send_f => {
                tracing::debug!("errors_to_sink send_f exited with {res:?}");
            }
            res = rcv_f => {
                tracing::debug!("errors_to_sink rcv_f exited with {res:?}");
            }
        }
        tracing::debug!("errors_to_sink thread exiting");
    });

    theirs
}

/// Convert the error type of a Pair, returning a new Pair.
///
/// The results of this combinator are:
///
/// (1) Errors into the original Pair's sink are intercepted and returned
///     in the new Pair's stream.
///
/// ```text
/// //                  ||
/// // orig_stream -->      --(map_err o into)--+--->   new_stream
/// //                  ||                      ^
/// // (Error = Ei)     ||                    (err)     (Error = Eo)
/// //                  ||                      |
/// // orig_sink   <--      <-----------(ok)----+----   new_sink
/// //                  ||
/// ```
///
/// (2) Errors from the original Pair's stream are converted (via Into)
///     to the new pair's error type.
///
/// Because of (1), the Error type of the new Sink is trivially changed.
/// The `conv` function handles converting errors from the original Stream.
pub fn errors_to_stream<S, R, Ei, Eo>(pair: impl Pair<S, R, Ei> + 'static) -> impl Pair<S, R, Eo>
where
    R: Send + 'static,
    S: Send + 'static,
    Ei: Into<Eo> + Send + 'static,
    Eo: Send + 'static,
{
    let (theirs, ours) = passthru_pair::<S, R, Eo>();
    let (p_sender, p_receiver) = pair.split();
    let (o_sender, o_receiver) = ours.split();
    let (e_sender, e_receiver) = mpsc::channel::<Result<R, Eo>>(0);

    // convert errors on the original Pair's stream into Ei
    let p_receiver = p_receiver.map_err(Into::into);

    // select(p_receiver, e_receiver).forward(o_sender)
    // o_receiver -> |m| if m.is_err() { e_sender.send(m) } else { o_sender.send(m) }
    tokio::spawn(async move {
        let send_f = select(e_receiver, p_receiver).map(Ok).forward(o_sender);
        let rcv_f =
            switch_convert_stream(o_receiver, |x| x.map(Ok).map_err(Err), p_sender, e_sender);

        tokio::select! {
            res = send_f => {
                tracing::debug!("errors_to_stream send_f exited with {res:?}");
            }
            res = rcv_f => {
                tracing::debug!("errors_to_stream rcv_f exited with {res:?}");
            }
        }
        tracing::debug!("errors_to_stream thread exiting");
    });

    theirs
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connector::passthru_pair,
        jrpc::{no_response, Error as JrpcError, Id},
        JsonRpcErr,
    };

    use futures::{FutureExt, SinkExt, StreamExt};
    use rstest::rstest;
    use std::pin::Pin;

    #[rstest]
    #[case::to_sink(0)]
    #[case::to_stream(1)]
    #[tokio::test]
    async fn test_errors(#[case] direction: usize) {
        let (mut errs, mut noerrs) = if direction == 0 {
            let (errs, noerrs) = passthru_pair::<usize, usize, JsonRpcErr>();
            let noerrs = errors_to_sink::<_, _, _, JrpcError>(noerrs);
            (
                Box::pin(errs) as Pin<Box<dyn Pair<usize, usize, JsonRpcErr>>>,
                Box::pin(noerrs) as Pin<Box<dyn Pair<usize, usize, JrpcError>>>,
            )
        } else {
            let (noerrs, errs) = passthru_pair::<usize, usize, JrpcError>();
            let errs = errors_to_stream::<_, _, _, JsonRpcErr>(errs);
            (
                Box::pin(errs) as Pin<Box<dyn Pair<usize, usize, JsonRpcErr>>>,
                Box::pin(noerrs) as Pin<Box<dyn Pair<usize, usize, JrpcError>>>,
            )
        };

        // Ok values pass from errs -> noerrs normally
        errs.send(Ok(0)).await.unwrap();
        assert_eq!(noerrs.next().await.transpose().unwrap(), Some(0));
        assert!(errs.next().now_or_never().is_none());

        // Ok values pass from noerrs -> errs normally
        noerrs.send(Ok(1)).await.unwrap();
        assert_eq!(errs.next().await.transpose().unwrap(), Some(1));
        assert!(noerrs.next().now_or_never().is_none());

        // Err values into errs come back out of errs
        errs.send(Err(no_response(None as Option<String>)))
            .await
            .unwrap();
        assert!(matches!(errs.next().await, Some(Err(JsonRpcErr::Jrpc(_)))));
        assert!(noerrs.next().now_or_never().is_none());

        // Err values into noerrs come out of errs
        noerrs
            .send(Err(JrpcError::new(Id::from(0), 0, "asdf", None)))
            .await
            .unwrap();
        assert!(matches!(errs.next().await, Some(Err(JsonRpcErr::Jrpc(_)))));
        assert!(noerrs.next().now_or_never().is_none());
    }
}
