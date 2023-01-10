// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! Copy transformer: Given a pair, make a copy of the input to two outputs

use futures::{stream::select, SinkExt, StreamExt};

use crate::{connector::passthru_pair, Pair};

/// Given a Pair, return two new Pairs that are duplicates of the original
pub fn copy<S, R>(pair: impl Pair<S, R> + 'static) -> (impl Pair<S, R>, impl Pair<S, R>)
where
    S: Send + 'static,
    R: Clone + Send + 'static,
{
    let (ret_left, left) = passthru_pair();
    let (ret_right, right) = passthru_pair();

    tokio::spawn(async move {
        let (left_snd, left_rcv) = left.split();
        let (right_snd, right_rcv) = right.split();
        let (pair_snd, pair_rcv) = pair.split();

        let send_f = select(left_rcv, right_rcv).map(Ok).forward(pair_snd);
        let rcv_f = pair_rcv.map(Ok).forward(right_snd.fanout(left_snd));

        tokio::select! {
            res = send_f => {
                tracing::debug!("copy send_f exited with {res:?}");
            }
            res = rcv_f => {
                tracing::debug!("copy rcv_f exited with {res:?}");
            }
        }

        tracing::debug!("copy thread exiting");
    });

    (ret_left, ret_right)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::JsonRpcErr;

    use futures::{
        future::{try_join, try_join3},
        FutureExt,
    };

    #[tokio::test]
    async fn test_copy() {
        let (mut testin, testout) = passthru_pair::<usize, usize, JsonRpcErr>();
        let (mut testout_1, mut testout_2) = copy(testout);

        // send() then next(), testin -> testout
        testin.send(Ok(0)).await.unwrap();
        assert_eq!(testout_1.next().await.transpose().unwrap(), Some(0));
        assert_eq!(testout_2.next().await.transpose().unwrap(), Some(0));

        // send() and next() concurrently, testin -> testout
        let sf = testin.send(Ok(1));
        let rf1 = testout_1.next().map(Ok);
        let rf2 = testout_2.next().map(Ok);
        let (_, recv1, recv2) = try_join3(sf, rf1, rf2).await.unwrap();
        assert_eq!(recv1.transpose().unwrap(), Some(1));
        assert_eq!(recv2.transpose().unwrap(), Some(1));

        // send() then next(), testout_1 -> testin
        testout_1.send(Ok(2)).await.unwrap();
        assert_eq!(testin.next().await.transpose().unwrap(), Some(2));
        assert!(testout_2.next().now_or_never().is_none());

        // send() then next(), testout_2 -> testin
        testout_2.send(Ok(3)).await.unwrap();
        assert_eq!(testin.next().await.transpose().unwrap(), Some(3));
        assert!(testout_1.next().now_or_never().is_none());

        // send() and next() concurrently, testout_1 -> testout
        let sf = testout_1.send(Ok(4));
        let rf = testin.next().map(Ok);
        let (_, recv) = try_join(sf, rf).await.unwrap();
        assert_eq!(recv.transpose().unwrap(), Some(4));
        assert!(testout_2.next().now_or_never().is_none());

        // send() and next() concurrently, testout_2 -> testout
        let sf = testout_2.send(Ok(5));
        let rf = testin.next().map(Ok);
        let (_, recv) = try_join(sf, rf).await.unwrap();
        assert_eq!(recv.transpose().unwrap(), Some(5));
        assert!(testout_1.next().now_or_never().is_none());

        // send() from testout_1, then testout_2; then read both at testin
        testout_1.send(Ok(6)).await.unwrap();
        testout_2.send(Ok(7)).await.unwrap();
        // no ordering guarantee!
        if let Some(x) = testin.next().await.transpose().unwrap() {
            assert!(x == 6 || x == 7);
            assert_eq!(testin.next().await.transpose().unwrap(), Some(13 - x));
        } else {
            unreachable!()
        }
        assert!(testin.next().now_or_never().is_none());
        assert!(testout_1.next().now_or_never().is_none());
        assert!(testout_2.next().now_or_never().is_none());
    }
}
