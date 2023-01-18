// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! A type-conversion transformer

use futures::future::ready;
use futures::{Future, FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::to_string;

use crate::Pair;

/// Apply a pair of synchronous conversions to a Pair, returning another Pair
///
/// Any errors in conversion will be returned from the Stream / sent into the Sink
pub fn convert<Si: Send, Ri, Ei: Send, So, Ro, Eo>(
    pair: impl Pair<Si, Ri, Ei>,
    fr: impl Fn(Result<Ri, Ei>) -> Result<Ro, Eo> + Send,
    fs: impl Fn(Result<So, Eo>) -> Result<Si, Ei> + Send,
) -> impl Pair<So, Ro, Eo> {
    pair.map(fr).with(move |x| ready(Ok(fs(x))))
}

/// Apply a pair of future-returning conversions to a Pair, returning another Pair
///
/// Any errors in conversion will be returned from the Stream / sent into the Sink
pub fn convert_fut<Si, Ri, Ei, So, Ro, Eo, Ffr, Ffs>(
    pair: impl Pair<Si, Ri, Ei>,
    fr: impl Fn(Result<Ri, Ei>) -> Ffr + Send,
    fs: impl Fn(Result<So, Eo>) -> Ffs + Send,
) -> impl Pair<So, Ro, Eo>
where
    Ffr: Future<Output = Result<Ro, Eo>> + Send,
    Ffs: Future<Output = Result<Si, Ei>> + Send,
{
    pair.then(fr).with(move |x| fs(x).map(Ok))
}

fn to_json<T, E>(s: Result<String, E>) -> Result<T, E>
where
    T: for<'a> Deserialize<'a> + Send,
    serde_json::Error: Into<E>,
{
    serde_json::from_str(&s?).map_err(Into::into)
}

fn from_json<T, E>(v: Result<T, E>) -> Result<String, E>
where
    T: Serialize + Send,
    serde_json::Error: Into<E>,
{
    to_string(&v?).map_err(Into::into)
}

/// Parses a stream of JSON strings into any type T which implements deserialize
pub fn json<Ti, To, Ei>(pair: impl Pair<String, String, Ei>) -> impl Pair<Ti, To, Ei>
where
    To: for<'a> Deserialize<'a> + Send,
    Ti: Serialize + Send,
    Ei: Send,
    serde_json::Error: Into<Ei>,
{
    convert(pair, to_json, from_json)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{connector::passthru_pair, JsonRpcErr};

    use serde_json::{json, Value};

    #[tokio::test]
    async fn test_json() {
        let (mut spair, jpair) = passthru_pair::<String, String, JsonRpcErr>();
        let mut jpair = json::<Value, Value, JsonRpcErr>(jpair);

        // do send before (not concurrently with) receive -- this should be possible!
        spair.send(Ok("\"asdf\"".to_owned())).await.unwrap();
        assert_eq!(jpair.next().await.transpose().unwrap(), Some(json!("asdf")));

        // do send before (not concurrently with) receive -- this should be possible!
        jpair.send(Ok(json!([1, 2, 3]))).await.unwrap();
        assert_eq!(
            spair.next().await.transpose().unwrap(),
            Some("[1,2,3]".to_owned())
        );
    }

    #[tokio::test]
    async fn test_json_fut() {
        let (mut spair, jpair) = passthru_pair();
        let to_json_fut = move |x| async { to_json::<Value, JsonRpcErr>(x) };
        let from_json_fut = move |x| async { from_json::<Value, JsonRpcErr>(x) };
        let mut jpair = Box::pin(convert_fut(jpair, to_json_fut, from_json_fut));

        // do send before (not concurrently with) receive -- this should be possible!
        spair.send(Ok("\"asdf\"".to_owned())).await.unwrap();
        assert_eq!(jpair.next().await.transpose().unwrap(), Some(json!("asdf")));

        // do send before (not concurrently with) receive -- this should be possible!
        jpair.send(Ok(json!([1, 2, 3]))).await.unwrap();
        assert_eq!(
            spair.next().await.transpose().unwrap(),
            Some("[1,2,3]".to_owned())
        );
    }
}
