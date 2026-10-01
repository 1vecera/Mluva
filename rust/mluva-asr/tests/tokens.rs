use flate2::read::GzDecoder;
use mluva_asr::tokens::{ParakeetTokens, WhisperTokens};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

type DecodeText = Box<dyn Fn(&[i64]) -> String>;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn check(model_id: &str) {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-tokens.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let directory = tempfile::tempdir().unwrap();
    let model = reference["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == model_id)
        .unwrap();
    for filename in if model_id == "whisper-tiny" {
        vec!["vocab.json", "added_tokens.json"]
    } else {
        vec!["vocab.txt"]
    } {
        let spec = model["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["name"] == filename)
            .unwrap();
        let prefix = if model_id == "whisper-tiny" {
            "whisper"
        } else {
            "parakeet"
        };
        let mut content = Vec::new();
        GzDecoder::new(fs::File::open(root.join(format!("{prefix}-{filename}.gz"))).unwrap())
            .take(spec["size"].as_u64().unwrap() + 1)
            .read_to_end(&mut content)
            .unwrap();
        assert_eq!(content.len() as u64, spec["size"].as_u64().unwrap());
        assert_eq!(
            hex(&Sha256::digest(&content)),
            spec["sha256"].as_str().unwrap()
        );
        fs::write(directory.path().join(filename), content).unwrap();
    }
    let decode: DecodeText = if model_id == "whisper-tiny" {
        let tokens = WhisperTokens::load(directory.path()).unwrap();
        assert!(tokens.language("unsupported-language").is_err());
        assert!(tokens.decode(&[-1]).is_err());
        Box::new(move |ids| tokens.decode(ids).unwrap())
    } else {
        let tokens = ParakeetTokens::load(directory.path()).unwrap();
        assert_eq!(tokens.size(), 8193);
        assert_eq!(tokens.blank, 8192);
        assert!(tokens.decode(&[-1]).is_err());
        Box::new(move |ids| tokens.decode(ids).unwrap())
    };
    let case = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["model"] == model_id)
        .unwrap();
    let mut digest = Sha256::new();
    for id in case["individual_ids"].as_array().unwrap() {
        let id = id.as_i64().unwrap();
        let text = decode(&[id]);
        digest.update(id.to_le_bytes());
        digest.update((text.len() as u64).to_le_bytes());
        digest.update(text.as_bytes());
    }
    assert_eq!(
        hex(&digest.finalize()),
        case["individual_digest"].as_str().unwrap()
    );
    for sequence in case["combinations"].as_array().unwrap() {
        let ids: Vec<_> = sequence["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_i64().unwrap())
            .collect();
        let text = decode(&ids);
        assert_eq!(
            hex(&Sha256::digest(text.as_bytes())),
            sequence["sha256"].as_str().unwrap(),
            "{model_id}/{ids:?}"
        );
        assert_eq!(
            text.chars().count() as u64,
            sequence["characters"].as_u64().unwrap(),
            "{model_id}/{ids:?}"
        );
    }
}

#[test]
fn whisper_bytes_utf8_and_special_tokens_match_released_decoder() {
    check("whisper-tiny");
}

#[test]
fn parakeet_space_and_word_boundaries_match_released_decoder() {
    check("parakeet-v3");
}
