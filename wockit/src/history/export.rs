//! Pure, deterministic history export formatting.

use chrono::SecondsFormat;

use super::HistorySample;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryExportFormat {
    Csv,
    Json,
}

impl HistoryExportFormat {
    pub const fn filename_extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv",
            Self::Json => "application/json",
        }
    }
}

/// Select a format and return its exact export bytes.
pub fn export_data(samples: &[HistorySample], format: HistoryExportFormat) -> Vec<u8> {
    match format {
        HistoryExportFormat::Csv => export_csv(samples).into_bytes(),
        HistoryExportFormat::Json => export_json(samples, true).into_bytes(),
    }
}

/// CSV with the portable header and CRLF after every row, including the final row.
pub fn export_csv(samples: &[HistorySample]) -> String {
    let mut output = String::from("timestamp,players_online\r\n");
    for sample in sorted(samples) {
        output.push_str(&timestamp(sample));
        output.push(',');
        output.push_str(&sample.count.to_string());
        output.push_str("\r\n");
    }
    output
}

/// JSON with Swift `JSONEncoder` key ordering and pretty-print spacing.
///
/// The returned document intentionally has no trailing newline.
pub fn export_json(samples: &[HistorySample], pretty_printed: bool) -> String {
    let samples = sorted(samples);
    if !pretty_printed {
        let rows = samples
            .iter()
            .map(|sample| {
                format!(
                    "{{\"players_online\":{},\"timestamp\":\"{}\"}}",
                    sample.count,
                    timestamp(sample)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        return format!("[{rows}]");
    }

    if samples.is_empty() {
        return "[]".to_owned();
    }
    let rows = samples
        .iter()
        .map(|sample| {
            format!(
                "  {{\n    \"players_online\" : {},\n    \"timestamp\" : \"{}\"\n  }}",
                sample.count,
                timestamp(sample)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!("[\n{rows}\n]")
}

fn sorted(samples: &[HistorySample]) -> Vec<&HistorySample> {
    let mut sorted: Vec<_> = samples.iter().collect();
    sorted.sort_by_key(|sample| (sample.date, sample.count));
    sorted
}

fn timestamp(sample: &HistorySample) -> String {
    sample.date.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn sample(seconds: i64, count: i64) -> HistorySample {
        HistorySample::new(Utc.timestamp_opt(seconds, 0).unwrap(), count)
    }

    #[test]
    fn csv_byte_vector_has_crlf_chronological_rows_and_final_crlf() {
        let bytes = export_data(
            &[sample(1_700_000_060, 104), sample(1_700_000_000, 91)],
            HistoryExportFormat::Csv,
        );
        assert_eq!(
            bytes,
            b"timestamp,players_online\r\n\
2023-11-14T22:13:20Z,91\r\n\
2023-11-14T22:14:20Z,104\r\n"
        );
    }

    #[test]
    fn json_byte_vector_matches_swift_sorted_key_spacing_without_newline() {
        let bytes = export_data(&[sample(1_700_000_000, 91)], HistoryExportFormat::Json);
        assert_eq!(
            bytes,
            b"[\n  {\n    \"players_online\" : 91,\n    \"timestamp\" : \"2023-11-14T22:13:20Z\"\n  }\n]"
        );
        assert_ne!(bytes.last(), Some(&b'\n'));
    }

    #[test]
    fn ties_sort_by_count_and_compact_json_has_no_whitespace() {
        let rows = [sample(1_700_000_000, 9), sample(1_700_000_000, 3)];
        assert_eq!(
            export_json(&rows, false),
            "[{\"players_online\":3,\"timestamp\":\"2023-11-14T22:13:20Z\"},{\"players_online\":9,\"timestamp\":\"2023-11-14T22:13:20Z\"}]"
        );
    }

    #[test]
    fn format_metadata_and_empty_documents_are_frozen() {
        assert_eq!(HistoryExportFormat::Csv.filename_extension(), "csv");
        assert_eq!(HistoryExportFormat::Csv.content_type(), "text/csv");
        assert_eq!(HistoryExportFormat::Json.filename_extension(), "json");
        assert_eq!(HistoryExportFormat::Json.content_type(), "application/json");
        assert_eq!(export_csv(&[]), "timestamp,players_online\r\n");
        assert_eq!(export_json(&[], false), "[]");
        assert_eq!(export_json(&[], true), "[]");
    }
}
