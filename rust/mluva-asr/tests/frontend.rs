use flate2::read::GzDecoder;
use mluva_asr::frontend::{Frontend, Kind};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

fn unpack(path: &Path, sha: &str, size: usize) -> Vec<u8> {
    let mut content = Vec::new();
    GzDecoder::new(fs::File::open(path).unwrap())
        .take(size as u64 + 1)
        .read_to_end(&mut content)
        .unwrap();
    assert_eq!(content.len(), size);
    let digest: String = Sha256::digest(&content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(digest, sha);
    content
}

#[test]
fn cpu_features_match_released_numpy_observations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-frontends.json")).unwrap();
    for resource in reference["resources"].as_array().unwrap() {
        let data = fs::read(
            root.join("../../resources")
                .join(resource["name"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(data.len() as u64, resource["size"].as_u64().unwrap());
        let digest: String = Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, resource["sha256"].as_str().unwrap());
    }
    let inference: Value =
        serde_json::from_str(include_str!("fixtures/released-onnx-inference.json")).unwrap();
    for resource in inference["runtime"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|resource| resource["archive"] == "asr")
    {
        let filename = Path::new(resource["member"].as_str().unwrap())
            .file_name()
            .unwrap();
        let data = fs::read(root.join("../../resources").join(filename)).unwrap();
        assert_eq!(data.len() as u64, resource["size"].as_u64().unwrap());
        let digest: String = Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, resource["sha256"].as_str().unwrap());
    }
    let whisper = Frontend::new(Kind::Whisper);
    let nemo = Frontend::new(Kind::Parakeet);
    for case in reference["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let frames = case["frames"].as_u64().unwrap() as usize;
        let pcm = unpack(
            &root.join(format!("{name}.pcm16le.gz")),
            case["pcm_sha256"].as_str().unwrap(),
            frames * 2,
        );
        let audio: Vec<_> = pcm
            .chunks_exact(2)
            .map(|bytes| f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0)
            .collect();
        let frontend = if case["engine"] == "whisper" {
            &whisper
        } else {
            &nemo
        };
        let actual = frontend.extract(&audio).unwrap();
        assert_eq!(
            serde_json::json!([1, actual.bands, actual.frames]),
            case["shape"],
            "{name}"
        );
        assert_eq!(
            actual.valid_frames,
            case["feature_length"].as_i64().unwrap(),
            "{name}"
        );
        let bytes = unpack(
            &root.join(format!("{name}.features.f32le.gz")),
            case["features_sha256"].as_str().unwrap(),
            actual.values.len() * 4,
        );
        let expected = bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()));
        let mut maximum = 0.0_f32;
        for (index, (actual, expected)) in actual.values.iter().copied().zip(expected).enumerate() {
            if expected.is_nan() {
                assert!(actual.is_nan(), "{name}[{index}]: expected NaN");
            } else if !expected.is_finite() {
                assert_eq!(actual, expected, "{name}[{index}]");
            } else {
                assert!(actual.is_finite(), "{name}[{index}]: expected finite");
                maximum = maximum.max((actual - expected).abs());
            }
        }
        eprintln!("{name}: largest absolute feature difference {maximum}");
        assert!(maximum <= 1e-5, "{name}: {maximum}");
    }
}

#[test]
fn frontend_bounds_audio_before_allocating_features() {
    let frontend = Frontend::new(Kind::Whisper);
    assert!(frontend.extract(&[f32::NAN]).is_err());
    assert!(frontend.extract(&[f32::INFINITY]).is_err());
    assert!(frontend.extract(&vec![0.0; 400_001]).is_err());
}
