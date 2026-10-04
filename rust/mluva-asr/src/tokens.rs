//! Released greedy-decoder vocabulary, UTF-8 and Unicode boundary rules.
use crate::{Error, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

pub struct WhisperTokens {
    tokens: BTreeMap<String, i64>,
    vocabulary: HashMap<i64, String>,
    bytes: HashMap<char, u8>,
    pub bos: i64,
    pub eos: i64,
}

impl WhisperTokens {
    pub fn load(directory: &Path) -> Result<Self> {
        let mut tokens: BTreeMap<String, i64> = serde_json::from_slice(
            &fs::read(directory.join("vocab.json")).map_err(|_| Error::Model)?,
        )
        .map_err(|_| Error::Model)?;
        let added: BTreeMap<String, i64> = serde_json::from_slice(
            &fs::read(directory.join("added_tokens.json")).map_err(|_| Error::Model)?,
        )
        .map_err(|_| Error::Model)?;
        tokens.extend(added);
        let vocabulary = tokens
            .iter()
            .map(|(token, id)| (*id, token.clone()))
            .collect();
        let mut extension = 256;
        let bytes = (0..=255_u8)
            .map(|byte| {
                let character =
                    if (33..=126).contains(&byte) || (161..=172).contains(&byte) || byte >= 174 {
                        char::from(byte)
                    } else {
                        let result = char::from_u32(extension).unwrap();
                        extension += 1;
                        result
                    };
                (character, byte)
            })
            .collect();
        Ok(Self {
            bos: *tokens.get("<|startoftranscript|>").ok_or(Error::Model)?,
            eos: *tokens.get("<|endoftext|>").ok_or(Error::Model)?,
            tokens,
            vocabulary,
            bytes,
        })
    }

    pub fn transcribe_prompt(&self, language_token: i64) -> Result<Vec<i64>> {
        Ok(vec![
            self.bos,
            language_token,
            *self.tokens.get("<|transcribe|>").ok_or(Error::Model)?,
            *self.tokens.get("<|notimestamps|>").ok_or(Error::Model)?,
        ])
    }

    pub fn language(&self, language: &str) -> Result<i64> {
        self.tokens
            .get(&format!("<|{language}|>"))
            .copied()
            .ok_or(Error::Language)
    }

    pub fn decode(&self, ids: &[i64]) -> Result<String> {
        let mut bytes = Vec::new();
        for id in ids {
            let token = self.vocabulary.get(id).ok_or(Error::Model)?;
            if token.starts_with("<|") {
                continue;
            }
            for character in token.chars() {
                bytes.push(*self.bytes.get(&character).ok_or(Error::Model)?);
            }
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.strip_prefix(' ').unwrap_or(&text).to_owned())
    }
}

pub struct ParakeetTokens {
    vocabulary: BTreeMap<i64, String>,
    pub blank: i64,
}

impl ParakeetTokens {
    pub fn load(directory: &Path) -> Result<Self> {
        let content = fs::read_to_string(directory.join("vocab.txt")).map_err(|_| Error::Model)?;
        let mut vocabulary = BTreeMap::new();
        for line in content.lines() {
            let (token, id) = line.split_once(' ').ok_or(Error::Model)?;
            vocabulary.insert(
                id.parse().map_err(|_| Error::Model)?,
                token.replace('▁', " "),
            );
        }
        let blank = *vocabulary
            .iter()
            .find(|(_, token)| token.as_str() == "<blk>")
            .ok_or(Error::Model)?
            .0;
        Ok(Self { vocabulary, blank })
    }
    pub fn size(&self) -> usize {
        self.vocabulary.len()
    }

    pub fn decode(&self, ids: &[i64]) -> Result<String> {
        let mut text = String::new();
        for id in ids {
            text.push_str(self.vocabulary.get(id).ok_or(Error::Model)?);
        }
        // Source regex: \A\s | \s\B | (\s)\b, using Python's Unicode word/space properties.
        let characters: Vec<_> = text.chars().collect();
        let mut result = String::new();
        for (index, character) in characters.iter().copied().enumerate() {
            if mluva_core::text::whitespace(character) {
                let boundary = characters
                    .get(index + 1)
                    .copied()
                    .is_some_and(mluva_core::text::word_character);
                if index > 0 && boundary {
                    result.push(' ');
                }
            } else {
                result.push(character);
            }
        }
        Ok(result)
    }
}
