use async_nats::jetstream;
use std::time::Duration;

pub const STREAM_NAME: &str = "EVENTS";

pub async fn ensure_streams(js: &jetstream::Context) -> Result<(), async_nats::Error> {
    js.get_or_create_stream(jetstream::stream::Config {
        name: STREAM_NAME.into(),
        subjects: vec!["events.>".into()],
        storage: jetstream::stream::StorageType::File,
        retention: jetstream::stream::RetentionPolicy::Limits,
        max_messages_per_subject: 1_000_000,
        discard: jetstream::stream::DiscardPolicy::Old,
        duplicate_window: Duration::from_secs(120),
        ..Default::default()
    })
    .await?;
    Ok(())
}
