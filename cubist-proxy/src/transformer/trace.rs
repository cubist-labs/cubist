use futures::{SinkExt, StreamExt};

use crate::Pair;

use std::fmt::Debug;

/// Logs all messages into and out of the pair, printing the label along with it.
pub fn debug<S, R, E>(label: &str, p: impl Pair<S, R, E>) -> impl Pair<S, R, E>
where
    S: Debug + Send,
    R: Debug + Send,
    E: Debug + Send,
{
    let label = label.to_owned();
    let label2 = label.to_owned();
    p.map(move |a| {
        tracing::debug!("{}:out: {:?}", label, a);
        a
    })
    .with(move |a| {
        let label = label2.clone();
        async move {
            tracing::debug!("{}:in: {:?}", label, a);
            Ok(a)
        }
    })
}
