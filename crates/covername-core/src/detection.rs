//! PII detection via regex-based rules.
//!
//! The rule engine scans text for patterns that indicate personally identifiable
//! information. It ships with built-in rules for common PII formats (SSN, phone,
//! email, credit card) and supports user-defined custom rules.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A single detection rule that matches PII patterns in text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Human-readable name for the rule.
    pub name: String,

    /// Optional description of what the rule detects.
    #[serde(default)]
    pub description: String,

    /// The regex pattern to match against text.
    pub pattern: String,

    /// The entity type assigned to matches (e.g., "SSN", "PHONE", "EMAIL").
    pub entity_type: String,

    /// Whether this rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Whether this is a built-in rule (not user-created).
    #[serde(default)]
    pub built_in: bool,

    /// When this rule was created (for custom rules).
    #[serde(default = "Utc::now")]
    pub created: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// A single PII detection found in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// The matched text.
    pub matched_text: String,

    /// The entity type (from the rule that matched).
    pub entity_type: String,

    /// The name of the rule that produced this detection.
    pub rule_name: String,

    /// Byte offset of the start of the match in the source text.
    pub start: usize,

    /// Byte offset of the end of the match in the source text.
    pub end: usize,

    /// Surrounding context (a snippet of text around the match).
    pub context: String,
}

/// The rule engine that scans text using a collection of rules.
pub struct RuleEngine {
    /// All rules (built-in + custom), with their compiled regex.
    rules: Vec<(Rule, Regex)>,
}

impl RuleEngine {
    /// Create a new rule engine with built-in default rules.
    ///
    /// # Errors
    ///
    /// Returns an error if any built-in rule has an invalid regex (should not happen).
    pub fn new() -> Result<Self> {
        let built_in = Self::built_in_rules();
        let mut rules = Vec::with_capacity(built_in.len());

        for rule in built_in {
            let regex = Regex::new(&rule.pattern).map_err(|e| Error::InvalidPattern {
                pattern: rule.pattern.clone(),
                reason: e.to_string(),
            })?;
            rules.push((rule, regex));
        }

        Ok(Self { rules })
    }

    /// Load custom rules from a JSON file and add them to the engine.
    ///
    /// If the file does not exist, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read/parsed,
    /// or if a custom rule has an invalid regex pattern.
    pub fn load_custom_rules(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let contents = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let custom_rules: Vec<Rule> = serde_json::from_str(&contents)?;

        for rule in custom_rules {
            if !rule.enabled {
                continue;
            }
            let regex = Regex::new(&rule.pattern).map_err(|e| Error::InvalidPattern {
                pattern: rule.pattern.clone(),
                reason: e.to_string(),
            })?;
            self.rules.push((rule, regex));
        }

        Ok(())
    }

    /// Scan text and return all detections.
    ///
    /// Runs every enabled rule against the text. Returns detections sorted
    /// by their start position. Filters out toll-free phone numbers and
    /// rejects address detections containing non-ASCII characters (OCR garbage).
    pub fn scan(&self, text: &str) -> Vec<Detection> {
        let mut detections = Vec::new();

        for (rule, regex) in &self.rules {
            for mat in regex.find_iter(text) {
                let matched_text = mat.as_str().to_string();

                // Skip toll-free phone numbers (800, 833, 844, 855, 866, 877, 888)
                if rule.entity_type == "PHONE" && Self::is_toll_free(&matched_text) {
                    continue;
                }

                // Reject ADDRESS detections containing non-ASCII characters (OCR garbage)
                if rule.entity_type == "ADDRESS" && Self::contains_non_address_chars(&matched_text)
                {
                    continue;
                }

                let context = Self::extract_context(text, mat.start(), mat.end());
                detections.push(Detection {
                    matched_text,
                    entity_type: rule.entity_type.clone(),
                    rule_name: rule.name.clone(),
                    start: mat.start(),
                    end: mat.end(),
                    context,
                });
            }
        }

        detections.sort_by_key(|d| d.start);
        detections
    }

    /// Check if a matched address text contains non-address characters.
    /// Rejects text with non-ASCII chars, control chars, or excessive garbage.
    fn contains_non_address_chars(text: &str) -> bool {
        // Reject if contains non-ASCII characters
        if !text.is_ascii() {
            return true;
        }
        // Reject if more than 50% non-alphanumeric-space characters
        let total = text.len();
        if total == 0 {
            return false;
        }
        let garbage_count = text
            .chars()
            .filter(|c| {
                !c.is_alphanumeric()
                    && !c.is_whitespace()
                    && *c != '-'
                    && *c != ','
                    && *c != '.'
                    && *c != '#'
            })
            .count();
        garbage_count * 2 > total
    }

    /// Check if a phone number string contains a toll-free area code.
    fn is_toll_free(phone: &str) -> bool {
        // Extract digits only
        let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
        // Get the area code (first 3 digits, or digits 2-4 if starts with country code 1)
        let area_code = if digits.len() == 11 && digits.starts_with('1') {
            &digits[1..4]
        } else if digits.len() >= 10 {
            &digits[0..3]
        } else {
            return false;
        };
        matches!(
            area_code,
            "800" | "833" | "844" | "855" | "866" | "877" | "888"
        )
    }

    /// Return a reference to all loaded rules.
    pub fn rules(&self) -> Vec<&Rule> {
        self.rules.iter().map(|(rule, _)| rule).collect()
    }

    /// Test a single pattern against text, returning matches.
    ///
    /// Useful for validating a new rule before saving it.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is not a valid regex.
    pub fn test_pattern(pattern: &str, text: &str) -> Result<Vec<Detection>> {
        let regex = Regex::new(pattern).map_err(|e| Error::InvalidPattern {
            pattern: pattern.to_string(),
            reason: e.to_string(),
        })?;

        let mut detections = Vec::new();
        for mat in regex.find_iter(text) {
            let context = Self::extract_context(text, mat.start(), mat.end());
            detections.push(Detection {
                matched_text: mat.as_str().to_string(),
                entity_type: String::from("TEST"),
                rule_name: String::from("test"),
                start: mat.start(),
                end: mat.end(),
                context,
            });
        }

        Ok(detections)
    }

    /// Extract a context snippet around a match (up to 40 chars on each side).
    fn extract_context(text: &str, start: usize, end: usize) -> String {
        crate::utils::extract_context(text, start, end)
    }

    /// The built-in default rules for common PII patterns.
    #[allow(clippy::too_many_lines)] // Declarative list of rules, not complex logic
    fn built_in_rules() -> Vec<Rule> {
        let now = Utc::now();
        vec![
            Rule {
                name: String::from("SSN"),
                description: String::from("US Social Security Number (XXX-XX-XXXX)"),
                pattern: String::from(r"\b\d{3}-\d{2}-\d{4}\b"),
                entity_type: String::from("SSN"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Phone (US)"),
                description: String::from("US phone number in common formats (excludes toll-free)"),
                pattern: String::from(r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b"),
                entity_type: String::from("PHONE"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Email"),
                description: String::from("Email addresses"),
                pattern: String::from(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"),
                entity_type: String::from("EMAIL"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Credit Card"),
                description: String::from("Credit card numbers (Visa, Mastercard, Amex, Discover)"),
                pattern: String::from(
                    r"\b(?:4\d{3}|5[1-5]\d{2}|3[47]\d{2}|6(?:011|5\d{2}))[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b",
                ),
                entity_type: String::from("CREDIT_CARD"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Account Number"),
                description: String::from("Account numbers following common label patterns"),
                pattern: String::from(r"(?i)(?:account|acct)[\s#:]*(\d[\d\s-]{4,20}\d)"),
                entity_type: String::from("ACCOUNT_NUMBER"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Long Number Sequence"),
                description: String::from(
                    "Standalone long number sequences (8-20 digits) that may be account numbers",
                ),
                pattern: String::from(r"\b\d{8,20}\b"),
                entity_type: String::from("ACCOUNT_NUMBER"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Street Address"),
                description: String::from("US street addresses with street type and optional unit"),
                pattern: String::from(
                    r"(?im)^\d{1,5}\s+[A-Za-z0-9 ]+(?:ST|STREET|AVE|AVENUE|BLVD|BOULEVARD|DR|DRIVE|LN|LANE|RD|ROAD|CT|COURT|PL|PLACE|WAY|CIR|CIRCLE)\b[, ]*(?:(?:APT|SUITE|STE|UNIT|#)\s*[A-Za-z0-9]*)?",
                ),
                entity_type: String::from("ADDRESS"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("City State Zip"),
                description: String::from("City, State ZIP code pattern"),
                pattern: String::from(
                    r"(?i)\b[A-Z][A-Za-z ]+,?\s+(?:AL|AK|AZ|AR|CA|CO|CT|DE|FL|GA|HI|ID|IL|IN|IA|KS|KY|LA|ME|MD|MA|MI|MN|MS|MO|MT|NE|NV|NH|NJ|NM|NY|NC|ND|OH|OK|OR|PA|RI|SC|SD|TN|TX|UT|VT|VA|WA|WV|WI|WY|DC)\s+\d{5}(?:-\d{4})?\b",
                ),
                entity_type: String::from("ADDRESS"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Date of Birth"),
                description: String::from("Date of birth patterns with DOB/Born labels"),
                pattern: String::from(
                    r"(?i)(?:d\.?o\.?b\.?|date of birth|born|birthday)[\s:]*(\d{1,2}[/\-\.]\d{1,2}[/\-\.]\d{2,4})",
                ),
                entity_type: String::from("DATE_OF_BIRTH"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Passport Number"),
                description: String::from(
                    "US passport numbers (9 alphanumeric characters with label)",
                ),
                pattern: String::from(r"(?i)(?:passport)[\s#:]*([A-Z0-9]{6,9})"),
                entity_type: String::from("ID_DOCUMENT"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Driver's License"),
                description: String::from("Driver's license numbers with common labels"),
                pattern: String::from(
                    r"(?i)(?:driver'?s?\s*(?:license|licence|lic)|DL)[\s#:]*([A-Z0-9]{4,15})",
                ),
                entity_type: String::from("ID_DOCUMENT"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Routing Number"),
                description: String::from("US bank routing numbers (9 digits with label)"),
                pattern: String::from(r"(?i)(?:routing|aba|ach)[\s#:]*(\d{9})\b"),
                entity_type: String::from("ACCOUNT_NUMBER"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("ITIN"),
                description: String::from(
                    "Individual Taxpayer Identification Number (9XX-XX-XXXX)",
                ),
                pattern: String::from(r"\b9\d{2}-\d{2}-\d{4}\b"),
                entity_type: String::from("SSN"),
                enabled: true,
                built_in: true,
                created: now,
            },
            Rule {
                name: String::from("Medicare/Insurance ID"),
                description: String::from("Medicare or health insurance ID numbers with labels"),
                pattern: String::from(
                    r"(?i)(?:medicare|medicaid|member|subscriber|policy|group)[\s#:]*(?:id|number|no)?[\s#:]*([A-Z0-9]{6,15})",
                ),
                entity_type: String::from("ID_DOCUMENT"),
                enabled: true,
                built_in: true,
                created: now,
            },
        ]
    }
}

/// Persistent store for custom (user-defined) rules.
///
/// Built-in rules are not stored here — they live in code.
/// This only manages user-created rules saved to disk.
pub struct CustomRuleStore {
    rules: Vec<Rule>,
    path: PathBuf,
}

impl CustomRuleStore {
    /// Load custom rules from a JSON file.
    ///
    /// Returns an empty store if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self> {
        let rules = if path.exists() {
            let contents = fs::read_to_string(path).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
            serde_json::from_str(&contents)?
        } else {
            Vec::new()
        };

        Ok(Self {
            rules,
            path: path.to_path_buf(),
        })
    }

    /// Save all custom rules to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.rules)?;
        fs::write(&self.path, json).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }

    /// Add a new custom rule.
    ///
    /// Validates the regex pattern before saving.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is invalid or saving fails.
    pub fn add(&mut self, name: &str, pattern: &str, entity_type: &str) -> Result<()> {
        // Validate the pattern compiles
        Regex::new(pattern).map_err(|e| Error::InvalidPattern {
            pattern: pattern.to_string(),
            reason: e.to_string(),
        })?;

        // Remove existing rule with same name (update semantics)
        self.rules.retain(|r| r.name != name);

        self.rules.push(Rule {
            name: name.to_string(),
            description: String::new(),
            pattern: pattern.to_string(),
            entity_type: entity_type.to_string(),
            enabled: true,
            built_in: false,
            created: Utc::now(),
        });

        self.save()
    }

    /// Remove a custom rule by name.
    ///
    /// Returns `true` if a rule was removed.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn remove(&mut self, name: &str) -> Result<bool> {
        let len_before = self.rules.len();
        self.rules.retain(|r| r.name != name);
        let removed = self.rules.len() < len_before;

        if removed {
            self.save()?;
        }

        Ok(removed)
    }

    /// Return all custom rules.
    pub fn list(&self) -> &[Rule] {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_ssn() {
        let engine = RuleEngine::new().unwrap();
        let text = "My SSN is 123-45-6789 and that's private.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].matched_text, "123-45-6789");
        assert_eq!(detections[0].entity_type, "SSN");
    }

    #[test]
    fn test_ssn_rejects_invalid() {
        let engine = RuleEngine::new().unwrap();
        // Too few digits
        let text = "Not an SSN: 12-34-5678 or 1234-56-7890.";
        let detections = engine.scan(text);
        assert!(detections.is_empty());
    }

    #[test]
    fn test_detect_phone_standard() {
        let engine = RuleEngine::new().unwrap();
        let text = "Call me at (555) 123-4567 anytime.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].entity_type, "PHONE");
    }

    #[test]
    fn test_detect_phone_formats() {
        let engine = RuleEngine::new().unwrap();

        let formats = [
            "555-123-4567",
            "555.123.4567",
            "(555) 123-4567",
            "5551234567",
            "+1 555-123-4567",
        ];

        for phone in formats {
            let text = format!("Phone: {phone} is the number.");
            let detections = engine.scan(&text);
            assert!(
                !detections.is_empty(),
                "Failed to detect phone format: {phone}"
            );
        }
    }

    #[test]
    fn test_detect_email() {
        let engine = RuleEngine::new().unwrap();
        let text = "Contact me at john.doe@example.com for details.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].matched_text, "john.doe@example.com");
        assert_eq!(detections[0].entity_type, "EMAIL");
    }

    #[test]
    fn test_detect_credit_card_visa() {
        let engine = RuleEngine::new().unwrap();
        let text = "Card: 4111-1111-1111-1111 expires 12/25.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].entity_type, "CREDIT_CARD");
    }

    #[test]
    fn test_detect_credit_card_mastercard() {
        let engine = RuleEngine::new().unwrap();
        let text = "Payment with 5500 0000 0000 0004.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].entity_type, "CREDIT_CARD");
    }

    #[test]
    fn test_detect_account_number() {
        let engine = RuleEngine::new().unwrap();
        let text = "Your Account #: 1234-5678-9012 is active.";
        let detections = engine.scan(text);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].entity_type, "ACCOUNT_NUMBER");
    }

    #[test]
    fn test_detect_account_number_variations() {
        let engine = RuleEngine::new().unwrap();

        let cases = [
            "Account: 123456789012",
            "Acct #1234567890",
            "ACCOUNT 9876-5432-1098",
        ];

        for case in cases {
            let detections = engine.scan(case);
            assert!(
                !detections.is_empty(),
                "Failed to detect account number in: {case}"
            );
        }
    }

    #[test]
    fn test_multiple_detections_sorted_by_position() {
        let engine = RuleEngine::new().unwrap();
        let text = "SSN: 123-45-6789, email: test@example.com, phone: 555-123-4567";
        let detections = engine.scan(text);

        assert!(detections.len() >= 3);
        // Verify sorted by start position
        for window in detections.windows(2) {
            assert!(window[0].start <= window[1].start);
        }
    }

    #[test]
    fn test_no_false_positives_on_clean_text() {
        let engine = RuleEngine::new().unwrap();
        let text = "This is a normal paragraph with no sensitive data. \
                    It talks about the weather and has numbers like 42 and 100.";
        let detections = engine.scan(text);
        assert!(detections.is_empty());
    }

    #[test]
    fn test_test_pattern() {
        let detections =
            RuleEngine::test_pattern(r"\b\d{3}-\d{2}-\d{4}\b", "My number is 123-45-6789.")
                .unwrap();

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].matched_text, "123-45-6789");
    }

    #[test]
    fn test_invalid_pattern_returns_error() {
        let result = RuleEngine::test_pattern(r"[invalid", "some text");
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_rule_store_add_and_list() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-rules.json");

        let mut store = CustomRuleStore::load(&path).unwrap();
        store
            .add(
                "Member ID",
                r"Member\s*ID\s*:?\s*(\w{6,12})",
                "ACCOUNT_NUMBER",
            )
            .unwrap();

        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "Member ID");
    }

    #[test]
    fn test_custom_rule_store_remove() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-rules.json");

        let mut store = CustomRuleStore::load(&path).unwrap();
        store.add("Test Rule", r"\d+", "NUMBER").unwrap();

        let removed = store.remove("Test Rule").unwrap();
        assert!(removed);
        assert!(store.list().is_empty());
    }

    #[test]
    fn test_custom_rule_store_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-rules.json");

        {
            let mut store = CustomRuleStore::load(&path).unwrap();
            store.add("My Rule", r"SECRET-\d+", "SECRET").unwrap();
        }

        let store = CustomRuleStore::load(&path).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "My Rule");
    }

    #[test]
    fn test_custom_rule_invalid_pattern_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-rules.json");

        let mut store = CustomRuleStore::load(&path).unwrap();
        let result = store.add("Bad Rule", r"[unclosed", "TEST");
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_with_custom_rules() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-rules.json");

        // Create a custom rule file
        let custom_rules = vec![Rule {
            name: String::from("Member ID"),
            description: String::from("Health member IDs"),
            pattern: String::from(r"(?i)member\s*id\s*:?\s*(\w{6,12})"),
            entity_type: String::from("MEMBER_ID"),
            enabled: true,
            built_in: false,
            created: Utc::now(),
        }];
        let json = serde_json::to_string_pretty(&custom_rules).unwrap();
        fs::write(&path, json).unwrap();

        // Load engine with custom rules
        let mut engine = RuleEngine::new().unwrap();
        engine.load_custom_rules(&path).unwrap();

        let text = "Your Member ID: ABC123XYZ is on file.";
        let detections = engine.scan(text);

        let member_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "MEMBER_ID")
            .collect();
        assert_eq!(member_detections.len(), 1);
    }
}
