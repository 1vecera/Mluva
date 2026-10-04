//! Synthetic, independently collected observations from the immutable 1.6.0 release.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use mluva_core::database::{StoreError, StoreResult};
use mluva_core::history::HistoryEntry;
use mluva_core::markdown::{self, MarkdownSpan};
use mluva_core::personalization::{
    self, DictionaryCaseBehavior, DictionaryReplacement, PersonalizationStore, Snippet,
};
use mluva_core::text_diff::{TextChange, text_changes};
use mluva_core::{text, vocabulary};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct TextCase {
    source: String,
    dictionary: Vec<DictionaryReplacement>,
    snippets: Vec<Snippet>,
    variables: BTreeMap<String, String>,
    expected: String,
}

#[test]
fn dictionary_case_boundaries_rule_order_and_spoken_snippets_match_the_release() {
    let cases: Vec<TextCase> =
        serde_json::from_str(include_str!("fixtures/personalization-text-cases.json")).unwrap();
    let mut differences = Vec::new();
    for case in cases {
        let actual = personalization::personalize_transcript(
            &case.source,
            &case.dictionary,
            &case.snippets,
            &case.variables,
        );
        if actual != case.expected {
            differences.push(format!(
                "{:?}: {:?} != {:?}",
                case.source, actual, case.expected
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences: {}",
        differences.len(),
        differences
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[derive(Deserialize)]
struct CaseConversion {
    source: String,
    upper: String,
    lower: String,
}

#[test]
fn full_unicode_casing_preserves_expansion_and_final_sigma_context() {
    let cases: Vec<CaseConversion> =
        serde_json::from_str(include_str!("fixtures/case-conversion-cases.json")).unwrap();
    let mut differences = Vec::new();
    for case in cases {
        let actual = (text::upper(&case.source), text::lower(&case.source));
        if actual != (case.upper.clone(), case.lower.clone()) {
            differences.push(format!(
                "{:?}: {:?} != {:?}",
                case.source,
                actual,
                (case.upper, case.lower)
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences: {}",
        differences.len(),
        differences
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[derive(Deserialize)]
struct IntegrityCase {
    source: String,
    candidate: String,
    vocabulary: Vec<String>,
    expected: Vec<String>,
}

#[test]
fn generative_integrity_keeps_exact_commands_identifiers_numbers_and_negation() {
    let cases: Vec<IntegrityCase> =
        serde_json::from_str(include_str!("fixtures/integrity-cases.json")).unwrap();
    let mut differences = Vec::new();
    for case in cases {
        let actual =
            personalization::integrity_violations(&case.source, &case.candidate, &case.vocabulary);
        if actual != case.expected {
            differences.push(format!(
                "{:?} -> {:?}: {:?} != {:?}",
                case.source, case.candidate, actual, case.expected
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "{}",
        differences
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[derive(Deserialize)]
struct LoadCase {
    label: String,
    json: String,
    accepted: bool,
    state: Value,
}

#[test]
fn existing_personalization_documents_load_compatibly_without_replacing_malformed_data() {
    let cases: Vec<LoadCase> =
        serde_json::from_str(include_str!("fixtures/personalization-load-cases.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (index, case) in cases.into_iter().enumerate() {
        let path = directory.path().join(format!("document-{index}.json"));
        fs::write(&path, &case.json).unwrap();
        let store = PersonalizationStore::new(&path);
        assert_eq!(
            store.persistence_error.is_none(),
            case.accepted,
            "{}: {:?}",
            case.label,
            store.persistence_error
        );
        let mut actual = store.state().document();
        for field in ["dictionary", "snippets", "styles"] {
            for (row, expected) in actual[field]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(case.state[field].as_array().unwrap())
            {
                if expected["id"] == "<generated>" {
                    uuid::Uuid::parse_str(row["id"].as_str().unwrap()).unwrap();
                    row["id"] = json!("<generated>");
                }
            }
        }
        assert_eq!(actual, case.state, "{}", case.label);
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            case.json,
            "{}: load changed the original document",
            case.label
        );
    }
}

#[derive(Deserialize)]
struct Trace {
    name: String,
    initial: Option<Value>,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    operation: Value,
    accepted: bool,
    result: Value,
    state: Value,
    file: Option<String>,
}

fn aliases(value: Value, known: &[(String, String)]) -> Value {
    match value {
        Value::String(value) => json!(
            known
                .iter()
                .find(|(_, id)| id == &value)
                .map(|(label, _)| label.clone())
                .unwrap_or(value)
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| aliases(value, known))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, aliases(value, known)))
                .collect(),
        ),
        other => other,
    }
}

fn operation(
    store: &mut PersonalizationStore,
    op: &Value,
    known: &[(String, String)],
) -> StoreResult<Value> {
    let application = op.get("application").and_then(Value::as_str);
    let remember = op["remember"].as_bool().unwrap_or(false);
    let id = op.get("id").and_then(Value::as_str).map(|value| {
        known
            .iter()
            .find(|(label, _)| label == value)
            .map(|(_, id)| id.as_str())
            .unwrap_or(value)
    });
    let required = |key: &str| {
        op.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| StoreError::Invalid(format!("missing {key}")))
    };
    let variables = BTreeMap::from([
        ("DATE".into(), "1. 10. 2026".into()),
        ("time".into(), "17:42".into()),
        ("weekday".into(), "Thursday".into()),
        ("date_time".into(), "literal $1 \\ path".into()),
    ]);
    match required("op")? {
        "dictionary" => serde_json::to_value(store.save_dictionary_replacement(
            required("spoken")?,
            required("written")?,
            application,
            if op["case"] == "matchSpoken" {
                DictionaryCaseBehavior::MatchSpoken
            } else {
                DictionaryCaseBehavior::Fixed
            },
        )?)
        .map_err(Into::into),
        "snippet" => serde_json::to_value(store.save_snippet(
            required("trigger")?,
            required("expansion")?,
            op.get("typed").and_then(Value::as_str),
            application,
        )?)
        .map_err(Into::into),
        "style" => {
            serde_json::to_value(store.save_style(required("name")?, required("instructions")?)?)
                .map_err(Into::into)
        }
        "update-style" => serde_json::to_value(store.update_style(
            id.unwrap(),
            required("name")?,
            required("instructions")?,
        )?)
        .map_err(Into::into),
        "delete-style" => {
            store.delete_style(id.unwrap())?;
            Ok(Value::Null)
        }
        "delete-dictionary" => {
            store.delete_dictionary_replacement(id.unwrap())?;
            Ok(Value::Null)
        }
        "delete-snippet" => {
            store.delete_snippet(id.unwrap())?;
            Ok(Value::Null)
        }
        "select-style" => {
            store.select_style(id, application, remember)?;
            Ok(Value::Null)
        }
        "mode" => {
            store.select_mode(required("mode")?, application, remember)?;
            Ok(Value::Null)
        }
        "dismiss" => {
            store.dismiss_vocabulary_suggestion(required("id")?)?;
            Ok(Value::Null)
        }
        "reload" => {
            *store = PersonalizationStore::new(&store.path);
            Ok(Value::Null)
        }
        "query" => Ok(json!({
            "dictionary": store.scoped_dictionary(application)?, "snippets":store.scoped_snippets(application)?,
            "context":store.scoped_recognition_context(application)?, "selected_style":store.selected_style(application, true)?,
            "mode":store.selected_mode(application,true,"dictation")?, "global_style":store.selected_style(application,false)?,
            "text":store.process_transcript("POST GRASS snippet review snippet new name",application,&variables)?,
            "typed":store.expand_typed_trigger(";review",application,&variables)?,
            "has_selection": application.map(|application| store.has_application_style_selection(application)).transpose()?.unwrap_or(false),
        })),
        _ => panic!("unknown fixture operation"),
    }
}

#[test]
fn saved_edits_scoped_selections_deletions_and_review_decisions_match_release_transactions() {
    let traces: Vec<Trace> =
        serde_json::from_str(include_str!("fixtures/personalization-store-cases.json")).unwrap();
    let root = tempfile::tempdir().unwrap();
    for trace in traces {
        let path = root.path().join(&trace.name).join("personalization.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(initial) = trace.initial {
            fs::write(
                &path,
                initial
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| serde_json::to_string(&initial).unwrap()),
            )
            .unwrap();
        }
        let mut store = PersonalizationStore::new(&path);
        let mut known: Vec<(String, String)> = Vec::new();
        for (index, step) in trace.steps.into_iter().enumerate() {
            let result = operation(&mut store, &step.operation, &known);
            assert_eq!(
                result.is_ok(),
                step.accepted,
                "{} step {index}: {:?}, {result:?}",
                trace.name,
                step.operation
            );
            let result = result.unwrap_or(Value::Null);
            if let Some(label) = step.operation.get("capture").and_then(Value::as_str) {
                let id = result
                    .get("id")
                    .or_else(|| result.get("identifier"))
                    .and_then(Value::as_str)
                    .unwrap();
                known.push((format!("<{label}>"), id.into()));
            }
            assert_eq!(
                aliases(result, &known),
                step.result,
                "{} step {index}: result",
                trace.name
            );
            assert_eq!(
                aliases(store.state().document(), &known),
                step.state,
                "{} step {index}: state",
                trace.name
            );
            let actual_file = fs::read_to_string(&path).ok().map(|mut content| {
                for (_, id) in &known {
                    let label = &known.iter().find(|(_, current)| current == id).unwrap().0;
                    content = content.replace(id, label);
                }
                content
            });
            assert_eq!(
                actual_file, step.file,
                "{} step {index}: persisted document",
                trace.name
            );
            if step.accepted && store.persistence_error.is_none() && path.exists() {
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
                assert_eq!(
                    fs::metadata(path.parent().unwrap())
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
            }
        }
    }
}

#[test]
fn failed_persistence_keeps_the_previous_in_memory_dictionary_and_private_document() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("personalization.json");
    let mut store = PersonalizationStore::new(&path);
    store
        .save_dictionary_replacement("phrase", "Original", None, DictionaryCaseBehavior::Fixed)
        .unwrap();
    let state = store.state().clone();
    let preserved = root.path().join("preserved.json");
    fs::rename(&path, &preserved).unwrap();
    let previous = fs::read(&preserved).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        store
            .save_dictionary_replacement(
                "phrase",
                "Replacement",
                None,
                DictionaryCaseBehavior::Fixed
            )
            .is_err()
    );
    assert_eq!(store.state(), &state);
    assert_eq!(fs::read(preserved).unwrap(), previous);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[derive(Deserialize)]
struct MarkdownCase {
    source: String,
    spans: Vec<MarkdownSpan>,
    visible: String,
    styles: Vec<MarkdownSpan>,
    wrap: bool,
}

#[test]
fn markdown_styling_projection_and_unicode_offsets_match_the_release() {
    let cases: Vec<MarkdownCase> =
        serde_json::from_str(include_str!("fixtures/markdown-cases.json")).unwrap();
    for case in cases {
        let spans = markdown::markdown_spans(&case.source);
        assert_eq!(spans, case.spans, "{:?}", case.source);
        assert_eq!(
            markdown::visible_markdown(&case.source, &spans),
            (case.visible, case.styles),
            "{:?}",
            case.source
        );
        assert_eq!(
            markdown::needs_character_wrapping(&case.source),
            case.wrap,
            "{:?}",
            case.source
        );
    }
}

#[test]
fn delimiter_heavy_documents_remain_source_preserving_with_bounded_output() {
    let source = "* ** _ __ *** ____ ` `` \\* ".repeat(10_000);
    let spans = markdown::markdown_spans(&source);
    let (visible, styles) = markdown::visible_markdown(&source, &spans);
    assert!(spans.len() <= source.chars().count());
    assert!(visible.len() <= source.len());
    let visible_characters = visible.chars().count();
    assert!(styles.iter().all(|span| span.end <= visible_characters));
    assert!(!markdown::needs_character_wrapping(&source));
    assert!(markdown::needs_character_wrapping(&"x".repeat(250_000)));
}

#[derive(Deserialize)]
struct CorrectionCase {
    source: String,
    corrected: String,
    expected: Option<(String, String)>,
}

#[test]
fn correction_suggestions_keep_the_same_focused_phrase_limits_and_exact_boundaries() {
    let cases: Vec<CorrectionCase> =
        serde_json::from_str(include_str!("fixtures/vocabulary-correction-cases.json")).unwrap();
    for case in cases {
        assert_eq!(
            vocabulary::focused_correction(&case.source, &case.corrected),
            case.expected,
            "{:?} -> {:?}",
            case.source,
            case.corrected
        );
    }
}

#[derive(Deserialize)]
struct SuggestionCase {
    entries: Vec<HistoryEntry>,
    dictionary: Vec<DictionaryReplacement>,
    dismissed: BTreeSet<String>,
    expected: Vec<vocabulary::VocabularySuggestion>,
}

#[test]
fn vocabulary_review_preserves_unicode_identity_scope_counts_order_and_dismissals() {
    let cases: Vec<SuggestionCase> =
        serde_json::from_str(include_str!("fixtures/vocabulary-suggestion-cases.json")).unwrap();
    for case in cases {
        assert_eq!(
            vocabulary::suggestions(&case.entries, &case.dictionary, &case.dismissed),
            case.expected
        );
    }
}

#[derive(Deserialize)]
struct DiffCase {
    previous: String,
    current: String,
    expected: Vec<TextChange>,
}

#[test]
fn visual_revisions_match_release_alignment_and_large_document_fallback() {
    let cases: Vec<DiffCase> =
        serde_json::from_str(include_str!("fixtures/text-diff-cases.json")).unwrap();
    for case in cases {
        assert_eq!(
            text_changes(&case.previous, &case.current),
            case.expected,
            "{:?} -> {:?}",
            case.previous,
            case.current
        );
    }
}
