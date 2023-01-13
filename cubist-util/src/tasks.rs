use core::future::Future;
use std::{iter::Iterator, time::Duration};

/// Retry a given function certain number of times or until it returns [`Ok`].
///
/// # Arguments
///
/// * `delays` - Iterator over [`Duration`] denoting how long to wait before each next retry
/// * `fun`    - Function to execute until it return [`Ok`] or `delays` are exhausted
pub async fn retry<TFn, TFut, TRes, TErr>(
    delays: impl IntoIterator<Item = Duration>,
    fun: TFn,
) -> Result<TRes, TErr>
where
    TFn: Fn() -> TFut,
    TFut: Future<Output = Result<TRes, TErr>>,
{
    let mut delays = delays.into_iter();
    loop {
        match fun().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if let Some(delay) = delays.next() {
                    tokio::time::sleep(delay).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[derive(Debug)]
    struct Error;

    use super::*;
    use std::iter::repeat;
    use std::time::Duration;

    #[tokio::test]
    async fn test_retry_ok() {
        // retry forever with a giant delay
        let delays = Box::new(repeat(Duration::from_secs(10000000000000)));
        retry(delays, ok).await.unwrap();
    }

    #[tokio::test]
    async fn test_retry_err() {
        // retry 3 times with 1us delay
        let delays = Box::new(repeat(Duration::from_micros(1)).take(3));
        assert!(retry(delays, err).await.is_err());
    }

    #[tokio::test]
    async fn test_retry_random() {
        // retry forever with 1us delay
        let delays = Box::new(repeat(Duration::from_micros(1)));
        retry(delays, random).await.unwrap();
    }

    async fn err() -> Result<(), Error> {
        Err(Error)
    }

    async fn ok() -> Result<(), Error> {
        Ok(())
    }

    async fn random() -> Result<(), Error> {
        if rand::random() {
            Ok(())
        } else {
            Err(Error)
        }
    }
}
