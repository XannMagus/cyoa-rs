//! Versioned, bounded save encoding and strict reconstruction; no filesystem effects.
use super::{
    dto::{GameSaveV1, SaveEnvelopeV1, SourceSave},
    migrations,
};
use cyoa_application::persistence::*;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveCodecErrorKind {
    Invalid,
    TooLarge,
    FutureVersion { found: u32, supported: u32 },
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid save at {location}: {message}")]
pub struct SaveCodecError {
    kind: SaveCodecErrorKind,
    location: String,
    message: String,
}
impl SaveCodecError {
    pub fn kind(&self) -> &SaveCodecErrorKind {
        &self.kind
    }
    pub fn location(&self) -> &str {
        &self.location
    }
    pub(super) fn invalid(location: &str, message: impl Into<String>) -> Self {
        Self {
            kind: SaveCodecErrorKind::Invalid,
            location: location.into(),
            message: message.into(),
        }
    }
    pub(super) fn future(found: u32, supported: u32) -> Self {
        Self {
            kind: SaveCodecErrorKind::FutureVersion { found, supported },
            location: "version".into(),
            message: format!(
                "Save version {found} is newer than supported version {supported}; use a newer cyoa."
            ),
        }
    }
    fn too_large(size: usize) -> Self {
        Self {
            kind: SaveCodecErrorKind::TooLarge,
            location: "$".into(),
            message: format!("save size {size} exceeds {MAX_SAVE_BYTES} byte bound"),
        }
    }
}

pub fn decode(
    bytes: &[u8],
    expected_id: &SaveId,
    copy: SaveCopy,
) -> Result<StoredGame, SaveCodecError> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(SaveCodecError::too_large(bytes.len()));
    }
    let value = crate::json::strict_value(bytes)
        .map_err(|e| SaveCodecError::invalid("$", e.to_string()))?;
    let value = migrations::upgrade(value)?;
    let mut unrecognized_fields = SourceSave::unrecognized_fields(&value);
    let envelope: SaveEnvelopeV1 =
        serde_ignored::deserialize(value, |path| unrecognized_fields.push(path.to_string()))
            .map_err(|e| SaveCodecError::invalid("$", e.to_string()))?;
    let id = SaveId::new(envelope.id).map_err(|e| SaveCodecError::invalid("id", e.to_string()))?;
    if id != *expected_id {
        return Err(SaveCodecError::invalid(
            "id",
            "document ID differs from requested slot",
        ));
    }
    let revision = SaveRevision::new(envelope.revision)
        .map_err(|e| SaveCodecError::invalid("revision", e.to_string()))?;
    let timestamp = OffsetDateTime::parse(&envelope.saved_at, &Rfc3339)
        .map_err(|e| SaveCodecError::invalid("saved_at", e.to_string()))?;
    if timestamp.offset() != UtcOffset::UTC {
        return Err(SaveCodecError::invalid(
            "saved_at",
            "save timestamp must be UTC",
        ));
    }
    let saved_at = SavedAt::new(timestamp.unix_timestamp(), timestamp.nanosecond())
        .map_err(|e| SaveCodecError::invalid("saved_at", e.to_string()))?;
    let source = envelope.source.into_domain()?;
    let game = envelope.game.into_domain()?;
    if envelope.title != game.world().outline().title().as_str() {
        return Err(SaveCodecError::invalid(
            "title",
            "metadata title differs from world title",
        ));
    }
    if envelope.turn_count != game.turns().len() as u64 {
        return Err(SaveCodecError::invalid(
            "turn_count",
            "metadata count differs from turn log",
        ));
    }
    let snapshot = SaveSnapshot::new(game, source)
        .map_err(|e| SaveCodecError::invalid("source", e.to_string()))?;
    Ok(StoredGame {
        snapshot,
        metadata: SaveMetadata {
            id,
            revision,
            saved_at,
        },
        stamp: stamp(bytes),
        copy,
        unrecognized_fields,
    })
}

pub fn encode(snapshot: &SaveSnapshot, metadata: &SaveMetadata) -> Result<Vec<u8>, SaveCodecError> {
    encode_bounded(snapshot, metadata, MAX_SAVE_BYTES)
}
fn encode_bounded(
    snapshot: &SaveSnapshot,
    metadata: &SaveMetadata,
    bound: usize,
) -> Result<Vec<u8>, SaveCodecError> {
    let timestamp = OffsetDateTime::from_unix_timestamp(metadata.saved_at.unix_seconds())
        .and_then(|t| t.replace_nanosecond(metadata.saved_at.nanoseconds()))
        .map_err(|e| SaveCodecError::invalid("saved_at", e.to_string()))?;
    let description = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z",
    )
    .expect("fixed valid timestamp format");
    let envelope = SaveEnvelopeV1 {
        version: migrations::CURRENT,
        id: metadata.id.as_str().into(),
        revision: metadata.revision.get(),
        title: snapshot.game().world().outline().title().as_str().into(),
        saved_at: timestamp
            .format(&description)
            .map_err(|e| SaveCodecError::invalid("saved_at", e.to_string()))?,
        turn_count: snapshot.game().turns().len() as u64,
        source: snapshot.source().into(),
        game: GameSaveV1::from(snapshot.game()),
    };
    // In-memory games may contain explicitly restored snapshots. Check the same
    // lossless aggregate constraints before producing a document we cannot read.
    let checked = GameSaveV1::from(snapshot.game()).into_domain()?;
    if checked != *snapshot.game() {
        return Err(SaveCodecError::invalid(
            "game",
            "save would change stored state",
        ));
    }
    let mut writer = BoundedWriter {
        bytes: vec![],
        bound,
        attempted: 0,
    };
    if let Err(e) = serde_json::to_writer_pretty(&mut writer, &envelope) {
        return Err(if writer.attempted > bound {
            SaveCodecError::too_large(writer.attempted)
        } else {
            SaveCodecError::invalid("$", e.to_string())
        });
    }
    writer
        .write_all(b"\n")
        .map_err(|_| SaveCodecError::too_large(writer.attempted))?;
    Ok(writer.bytes)
}
pub fn stamp(bytes: &[u8]) -> ContentStamp {
    ContentStamp::new(Sha256::digest(bytes).into())
}
struct BoundedWriter {
    bytes: Vec<u8>,
    bound: usize,
    attempted: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.attempted = self.bytes.len().saturating_add(bytes.len());
        if self.attempted > self.bound {
            return Err(io::Error::other("save size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_writer_rejects_a_document_or_final_newline_that_exceeds_the_limit() {
        let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
        let stored = decode(
            include_bytes!("../../tests/fixtures/saves/v1-minimal.json"),
            &id,
            SaveCopy::Primary,
        )
        .unwrap();
        let expected = encode(&stored.snapshot, &stored.metadata).unwrap();
        assert_eq!(
            encode_bounded(&stored.snapshot, &stored.metadata, expected.len()).unwrap(),
            expected
        );
        for bound in [0, 1, expected.len() - 1, expected.len() - 2] {
            assert_eq!(
                encode_bounded(&stored.snapshot, &stored.metadata, bound)
                    .unwrap_err()
                    .kind(),
                &SaveCodecErrorKind::TooLarge
            );
        }
    }
    #[test]
    fn canonical_writer_timestamps_cover_utc_year_boundaries_and_millisecond_precision() {
        let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
        let mut stored = decode(
            include_bytes!("../../tests/fixtures/saves/v1-minimal.json"),
            &id,
            SaveCopy::Primary,
        )
        .unwrap();
        for (seconds, nanos, expected) in [
            (-62_135_596_800, 0, "0001-01-01T00:00:00.000Z"),
            (253_402_300_799, 999_999_999, "9999-12-31T23:59:59.999Z"),
            (0, 123_456_789, "1970-01-01T00:00:00.123Z"),
        ] {
            stored.metadata.saved_at = SavedAt::new(seconds, nanos).unwrap();
            let bytes = encode(&stored.snapshot, &stored.metadata).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["saved_at"], expected);
            assert_eq!(
                decode(&bytes, &id, SaveCopy::Primary).unwrap().metadata,
                stored.metadata
            );
        }
    }
}
