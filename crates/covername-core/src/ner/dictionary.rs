//! Dictionary-based named entity detector.
//!
//! Uses a heuristic approach: capitalized words that don't appear in a list of
//! common English words are likely proper nouns (person names, places, etc.).
//! This avoids needing an ML model while catching many obvious person names.

use std::collections::HashSet;

use crate::detection::Detection;
use crate::ner::NerDetector;

/// A heuristic NER detector that uses a common-word dictionary to identify
/// likely person names by exclusion.
///
/// The logic: if a sequence of 2-3 capitalized words appears mid-sentence and
/// none of those words are common English words, the sequence is likely a
/// person name or proper noun.
pub struct DictionaryDetector {
    common_words: HashSet<&'static str>,
    organization_words: HashSet<&'static str>,
}

impl DictionaryDetector {
    /// Create a new dictionary detector with the built-in common word list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            common_words: Self::build_common_words(),
            organization_words: Self::build_organization_words(),
        }
    }

    /// Detect person names and address patterns in text.
    fn detect_names(&self, text: &str) -> Vec<Detection> {
        let mut detections = Vec::new();

        for (line_start, line) in LineIterator::new(text) {
            self.detect_names_in_line(text, line, line_start, &mut detections);
            Self::detect_addresses_in_line(text, line, line_start, &mut detections);
        }

        detections
    }

    /// Check if a matched text contains characters that invalidate it as a name.
    /// Rejects sequences containing periods, em-dashes, ampersands, non-ASCII, digits,
    /// or already-masked patterns like "XXXX".
    fn contains_invalid_name_chars(text: &str) -> bool {
        // Reject if contains non-ASCII characters
        if !text.is_ascii() {
            return true;
        }
        // Reject if contains periods, em-dashes, ampersands, or other special chars
        if text.contains('.')
            || text.contains('\u{2014}')
            || text.contains('&')
            || text.contains('\u{00AE}')
        {
            return true;
        }
        // Reject if contains already-masked patterns
        if text.contains("XXXX") || text.contains("xxxx") {
            return true;
        }
        false
    }

    /// Check if an ALL CAPS word is invalid as part of a name.
    /// Rejects words with digits, special characters, or that are all consonants (ticker-like).
    fn is_invalid_all_caps_word(word: &str) -> bool {
        // Reject if contains digits
        if word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        // Reject if contains special characters
        if word.chars().any(|c| !c.is_ascii_alphabetic()) {
            return true;
        }
        // Reject if all consonants (likely a ticker like SMHX, VTSAX)
        let vowels = ['A', 'E', 'I', 'O', 'U'];
        if word.len() >= 2 && word.chars().all(|c| !vowels.contains(&c)) {
            return true;
        }
        false
    }

    /// Detect capitalized word sequences that are likely person names within a line.
    fn detect_names_in_line(
        &self,
        full_text: &str,
        line: &str,
        line_start: usize,
        detections: &mut Vec<Detection>,
    ) {
        let words: Vec<WordInfo> = Self::tokenize_line(line);

        let mut i = 0;
        while i < words.len() {
            // For Title Case detection, skip words at the start of a sentence
            // (they're capitalized by grammar, not because they're names)
            // But still check ALL CAPS sequences regardless of position
            if !words[i].is_sentence_start && self.is_name_candidate(&words[i]) {
                let seq_end = self.find_name_sequence_end(&words, i);
                let seq_len = seq_end - i;

                if seq_len >= 2 && !self.sequence_contains_org_word(&words, i, seq_end) {
                    let start_byte = line_start + words[i].byte_offset;
                    let last_word = &words[seq_end - 1];
                    let end_byte = line_start + last_word.byte_offset + last_word.text.len();
                    let matched_text = &full_text[start_byte..end_byte];

                    // Reject sequences with invalid characters
                    if Self::contains_invalid_name_chars(matched_text) {
                        i = seq_end;
                        continue;
                    }

                    let context = Self::extract_context(full_text, start_byte, end_byte);

                    detections.push(Detection {
                        matched_text: matched_text.to_string(),
                        entity_type: String::from("PERSON"),
                        rule_name: String::from("NER (dictionary)"),
                        start: start_byte,
                        end: end_byte,
                        context,
                    });

                    i = seq_end;
                    continue;
                }
            }

            // Look for ALL CAPS sequences (2-3 words) that could be person names
            // Note: we don't skip sentence starts for ALL CAPS because financial
            // statements often put names on their own line in ALL CAPS
            if is_all_caps(words[i].text) && !Self::is_invalid_all_caps_word(words[i].text) {
                let seq_end = self.find_all_caps_name_end(&words, i);
                let seq_len = seq_end - i;

                // Reject ALL CAPS sequences longer than 3 words (likely fund names/headers)
                if (2..=3).contains(&seq_len)
                    && !self.sequence_contains_org_word(&words, i, seq_end)
                {
                    // Check that no word in the sequence is invalid
                    let has_invalid_word = words[i..seq_end]
                        .iter()
                        .any(|w| Self::is_invalid_all_caps_word(w.text));

                    if !has_invalid_word {
                        let start_byte = line_start + words[i].byte_offset;
                        let last_word = &words[seq_end - 1];
                        let end_byte = line_start + last_word.byte_offset + last_word.text.len();
                        let matched_text = &full_text[start_byte..end_byte];

                        // Reject sequences with invalid characters
                        if Self::contains_invalid_name_chars(matched_text) {
                            i = seq_end;
                            continue;
                        }

                        let context = Self::extract_context(full_text, start_byte, end_byte);

                        detections.push(Detection {
                            matched_text: matched_text.to_string(),
                            entity_type: String::from("PERSON"),
                            rule_name: String::from("NER (dictionary)"),
                            start: start_byte,
                            end: end_byte,
                            context,
                        });

                        i = seq_end;
                        continue;
                    }
                }
            }

            i += 1;
        }
    }

    /// Detect address-like patterns: number followed by capitalized street name.
    fn detect_addresses_in_line(
        full_text: &str,
        line: &str,
        line_start: usize,
        detections: &mut Vec<Detection>,
    ) {
        let words: Vec<WordInfo> = Self::tokenize_line(line);

        let mut i = 0;
        while i < words.len() {
            // Look for a number followed by capitalized words (street name)
            if words[i].text.chars().all(|c| c.is_ascii_digit()) && !words[i].text.is_empty() {
                let street_start = i + 1;
                if street_start < words.len() && is_capitalized(words[street_start].text) {
                    // Find end of street name (1-3 capitalized words after the number)
                    let mut street_end = street_start + 1;
                    while street_end < words.len()
                        && street_end < street_start + 3
                        && is_capitalized(words[street_end].text)
                    {
                        street_end += 1;
                    }

                    if street_end > street_start {
                        let start_byte = line_start + words[i].byte_offset;
                        let last_word = &words[street_end - 1];
                        let end_byte = line_start + last_word.byte_offset + last_word.text.len();
                        let matched_text = &full_text[start_byte..end_byte];

                        // Skip if contains non-ASCII or garbage characters
                        if !matched_text.is_ascii()
                            || matched_text.contains('€')
                            || matched_text.contains('®')
                            || matched_text.contains('&')
                        {
                            i = street_end;
                            continue;
                        }

                        // Avoid detecting things already detected as names
                        let context = Self::extract_context(full_text, start_byte, end_byte);

                        detections.push(Detection {
                            matched_text: matched_text.to_string(),
                            entity_type: String::from("ADDRESS"),
                            rule_name: String::from("NER (dictionary)"),
                            start: start_byte,
                            end: end_byte,
                            context,
                        });

                        i = street_end;
                        continue;
                    }
                }
            }

            i += 1;
        }
    }

    /// Check if a word is a candidate for being part of a name.
    fn is_name_candidate(&self, word: &WordInfo) -> bool {
        is_capitalized(word.text) && !self.is_common_word(word.text)
    }

    /// Find the end index of a consecutive name sequence starting at `start`.
    /// Returns at most 3 words from start.
    fn find_name_sequence_end(&self, words: &[WordInfo], start: usize) -> usize {
        let max_len = 3;
        let mut end = start + 1;

        while end < words.len() && end < start + max_len {
            if !self.is_name_candidate(&words[end]) {
                break;
            }
            end += 1;
        }

        end
    }

    /// Find the end index of a consecutive ALL CAPS name sequence starting at `start`.
    /// Returns at most 3 words from start.
    fn find_all_caps_name_end(&self, words: &[WordInfo], start: usize) -> usize {
        let max_len = 3;
        let mut end = start + 1;

        while end < words.len() && end < start + max_len {
            if !is_all_caps(words[end].text)
                || self.is_common_word(words[end].text)
                || Self::is_invalid_all_caps_word(words[end].text)
            {
                break;
            }
            end += 1;
        }

        end
    }

    /// Check if any word in the sequence [start..end) is a known organization word.
    fn sequence_contains_org_word(&self, words: &[WordInfo], start: usize, end: usize) -> bool {
        words[start..end]
            .iter()
            .any(|w| self.is_organization_word(w.text))
    }

    /// Check if a word (case-insensitive) is a known organization/company word.
    fn is_organization_word(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.organization_words.contains(lower.as_str())
    }

    /// Check if a word (case-insensitive) is in the common words set.
    fn is_common_word(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.common_words.contains(lower.as_str())
    }

    /// Tokenize a line into words with their byte offsets and sentence-start flags.
    fn tokenize_line(line: &str) -> Vec<WordInfo<'_>> {
        let mut words = Vec::new();
        let mut after_sentence_end = true; // Start of line = start of sentence
        let mut chars = line.char_indices().peekable();

        while let Some(&(byte_offset, ch)) = chars.peek() {
            if ch.is_whitespace() {
                chars.next();
                continue;
            }

            // Collect a word
            let word_start = byte_offset;
            let mut word_end = byte_offset;
            while let Some(&(idx, c)) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word_end = idx + c.len_utf8();
                chars.next();
            }

            let word_text = &line[word_start..word_end];

            // Strip trailing punctuation for analysis but keep offset accurate
            let stripped = word_text.trim_end_matches(|c: char| c.is_ascii_punctuation());

            if !stripped.is_empty() {
                words.push(WordInfo {
                    text: stripped,
                    byte_offset: word_start,
                    is_sentence_start: after_sentence_end,
                });
            }

            // Check if this word ends a sentence
            after_sentence_end =
                word_text.ends_with('.') || word_text.ends_with('!') || word_text.ends_with('?');
        }

        words
    }

    /// Extract a context snippet around a match.
    fn extract_context(text: &str, start: usize, end: usize) -> String {
        crate::utils::extract_context(text, start, end)
    }

    /// Build the set of common English words used for exclusion.
    #[allow(clippy::too_many_lines)]
    fn build_common_words() -> HashSet<&'static str> {
        HashSet::from([
            // Articles & determiners
            "a",
            "an",
            "the",
            "this",
            "that",
            "these",
            "those",
            "my",
            "your",
            "his",
            "her",
            "its",
            "our",
            "their",
            "some",
            "any",
            "no",
            "every",
            "each",
            "all",
            "both",
            "few",
            "many",
            "much",
            "several",
            "such",
            "what",
            "which",
            "whose",
            // Pronouns
            "i",
            "me",
            "we",
            "us",
            "you",
            "he",
            "him",
            "she",
            "it",
            "they",
            "them",
            "myself",
            "yourself",
            "himself",
            "herself",
            "itself",
            "ourselves",
            "themselves",
            "who",
            "whom",
            "whoever",
            "whatever",
            "something",
            "anything",
            "nothing",
            "everything",
            "someone",
            "anyone",
            "everyone",
            "nobody",
            // Prepositions
            "in",
            "on",
            "at",
            "to",
            "for",
            "with",
            "from",
            "by",
            "about",
            "into",
            "through",
            "during",
            "before",
            "after",
            "above",
            "below",
            "between",
            "under",
            "over",
            "up",
            "down",
            "out",
            "off",
            "against",
            "among",
            "along",
            "across",
            "around",
            "behind",
            "beside",
            "beyond",
            "near",
            "toward",
            "upon",
            "within",
            "without",
            // Conjunctions
            "and",
            "but",
            "or",
            "nor",
            "so",
            "yet",
            "for",
            "because",
            "although",
            "while",
            "since",
            "unless",
            "until",
            "though",
            "whereas",
            "whether",
            "if",
            "then",
            "else",
            // Common verbs
            "is",
            "am",
            "are",
            "was",
            "were",
            "be",
            "been",
            "being",
            "have",
            "has",
            "had",
            "having",
            "do",
            "does",
            "did",
            "doing",
            "will",
            "would",
            "shall",
            "should",
            "may",
            "might",
            "can",
            "could",
            "must",
            "need",
            "dare",
            "ought",
            "get",
            "got",
            "getting",
            "go",
            "goes",
            "went",
            "gone",
            "going",
            "come",
            "came",
            "coming",
            "make",
            "made",
            "making",
            "take",
            "took",
            "taken",
            "taking",
            "give",
            "gave",
            "given",
            "see",
            "saw",
            "seen",
            "know",
            "knew",
            "known",
            "think",
            "thought",
            "say",
            "said",
            "tell",
            "told",
            "find",
            "found",
            "want",
            "wanted",
            "look",
            "looked",
            "use",
            "used",
            "work",
            "worked",
            "call",
            "called",
            "try",
            "tried",
            "ask",
            "asked",
            "put",
            "keep",
            "kept",
            "let",
            "begin",
            "began",
            "seem",
            "seemed",
            "help",
            "helped",
            "show",
            "showed",
            "hear",
            "heard",
            "play",
            "played",
            "run",
            "ran",
            "move",
            "moved",
            "live",
            "lived",
            "believe",
            "hold",
            "held",
            "bring",
            "brought",
            "happen",
            "write",
            "wrote",
            "provide",
            "sit",
            "sat",
            "stand",
            "stood",
            "lose",
            "lost",
            "pay",
            "paid",
            "meet",
            "met",
            "include",
            "continue",
            "set",
            "learn",
            "change",
            "lead",
            "led",
            "understand",
            "watch",
            "follow",
            "stop",
            "create",
            "speak",
            "spoke",
            "read",
            "allow",
            "add",
            "spend",
            "spent",
            "grow",
            "grew",
            "open",
            "opened",
            "walk",
            "walked",
            "win",
            "won",
            "offer",
            "remember",
            "consider",
            "appear",
            "buy",
            "bought",
            "wait",
            "serve",
            "die",
            "send",
            "sent",
            "expect",
            "build",
            "built",
            "stay",
            "fall",
            "fell",
            "cut",
            "reach",
            "kill",
            "remain",
            // Common adjectives
            "good",
            "new",
            "first",
            "last",
            "long",
            "great",
            "little",
            "own",
            "other",
            "old",
            "right",
            "big",
            "high",
            "different",
            "small",
            "large",
            "next",
            "early",
            "young",
            "important",
            "public",
            "bad",
            "same",
            "able",
            "free",
            "true",
            "false",
            "full",
            "special",
            "easy",
            "clear",
            "recent",
            "sure",
            "real",
            "left",
            "late",
            "hard",
            "major",
            "better",
            "best",
            "possible",
            "whole",
            "certain",
            "open",
            "low",
            // Common nouns
            "time",
            "year",
            "people",
            "way",
            "day",
            "man",
            "woman",
            "child",
            "world",
            "life",
            "hand",
            "part",
            "place",
            "case",
            "week",
            "company",
            "system",
            "program",
            "question",
            "work",
            "government",
            "number",
            "night",
            "point",
            "home",
            "water",
            "room",
            "mother",
            "area",
            "money",
            "story",
            "fact",
            "month",
            "lot",
            "right",
            "study",
            "book",
            "eye",
            "job",
            "word",
            "business",
            "issue",
            "side",
            "kind",
            "head",
            "house",
            "service",
            "friend",
            "father",
            "power",
            "hour",
            "game",
            "line",
            "end",
            "member",
            "law",
            "car",
            "city",
            "community",
            "name",
            "president",
            "team",
            "minute",
            "idea",
            "body",
            "information",
            "back",
            "parent",
            "face",
            "others",
            "level",
            "office",
            "door",
            "health",
            "person",
            "art",
            "war",
            "history",
            "party",
            "result",
            "change",
            "morning",
            "reason",
            "research",
            "girl",
            "guy",
            "moment",
            "air",
            "teacher",
            "force",
            "education",
            // Common adverbs
            "not",
            "also",
            "very",
            "often",
            "however",
            "too",
            "usually",
            "really",
            "already",
            "always",
            "never",
            "sometimes",
            "still",
            "just",
            "now",
            "here",
            "there",
            "where",
            "when",
            "how",
            "why",
            "well",
            "back",
            "even",
            "only",
            "then",
            "again",
            "once",
            "more",
            "less",
            "quite",
            "rather",
            "almost",
            "ever",
            "enough",
            "far",
            "perhaps",
            "today",
            "together",
            "soon",
            "away",
            // Other common words
            "like",
            "just",
            "over",
            "also",
            "than",
            "very",
            "there",
            "about",
            "more",
            "one",
            "two",
            "three",
            "four",
            "five",
            "six",
            "seven",
            "eight",
            "nine",
            "ten",
            "first",
            "second",
            "third",
            "new",
            "old",
            "big",
            "small",
            "may",
            "no",
            "yes",
            "not",
            // Words that often start sentences but aren't names
            "the",
            "however",
            "therefore",
            "furthermore",
            "meanwhile",
            "nevertheless",
            "consequently",
            "additionally",
            "moreover",
            "finally",
            "thus",
            "hence",
            "accordingly",
            "indeed",
            "certainly",
            "obviously",
            "clearly",
            "apparently",
            "unfortunately",
            "fortunately",
            "surprisingly",
            "interestingly",
            // Address-related common words (to avoid false positives)
            "street",
            "avenue",
            "road",
            "drive",
            "lane",
            "court",
            "place",
            "boulevard",
            "way",
            "circle",
            "trail",
            "path",
            "park",
            // Business / document words
            "account",
            "balance",
            "transaction",
            "transfer",
            "payment",
            "deposit",
            "statement",
            "history",
            "total",
            "amount",
            "date",
            "credit",
            "debit",
            "reference",
            "status",
            "direct",
            // Institutional / organizational words
            "national",
            "bank",
            "federal",
            "state",
            "city",
            "county",
            "department",
            "corp",
            "corporation",
            "inc",
            "ltd",
            "llc",
            "university",
            "college",
            "school",
            "hospital",
            "center",
            "institute",
            "association",
            "foundation",
            "international",
            "global",
            "american",
            "western",
            "eastern",
            "northern",
            "southern",
            "central",
            "general",
            "royal",
            "united",
            "first",
            "second",
            "third",
        ])
    }

    /// Build the set of organization/company words used to filter out false positive names.
    #[allow(clippy::too_many_lines)]
    fn build_organization_words() -> HashSet<&'static str> {
        HashSet::from([
            // Financial institutions and terms
            "vanguard",
            "fidelity",
            "schwab",
            "finra",
            "sipc",
            "fdic",
            "sec",
            "nyse",
            "nasdaq",
            "brokerage",
            "corporation",
            "services",
            "marketing",
            "associates",
            "holdings",
            "capital",
            "investments",
            "securities",
            "fund",
            "trust",
            "bank",
            "insurance",
            "advisory",
            "advisors",
            "financial",
            "mutual",
            "exchange",
            "trading",
            "wealth",
            "management",
            // Investment fund terms
            "admiral",
            "index",
            "market",
            "etf",
            "stock",
            "bond",
            "treasury",
            "money",
            "sweep",
            "cl",
            "semiconductor",
            "vaneck",
            "fabless",
            "shares",
            "equity",
            "reit",
            "commodities",
            "futures",
            "options",
            "allocation",
            "benchmark",
            "composite",
            "yield",
            "maturity",
            "duration",
            "prospectus",
            // Common corporate suffixes/words
            "inc",
            "llc",
            "corp",
            "ltd",
            "company",
            "co",
            "group",
            "international",
            "national",
            "federal",
            "american",
            "institute",
            "foundation",
            "association",
            "department",
            "university",
            "hospital",
            "medical",
            "center",
            "committee",
            // Broker/dealer terms
            "broker",
            "dealer",
            "ascensus",
            "custodian",
            "fiduciary",
            "underwriter",
            // Product-like / document words
            "personal",
            "investor",
            "account",
            "statement",
            "summary",
            "report",
            "portfolio",
            "premium",
            "basic",
            "standard",
            "professional",
            "total",
            "dividend",
            "growth",
            "income",
            "balanced",
            "retirement",
            "target",
            // Financial document column headers / common terms
            "quantity",
            "price",
            "cost",
            "basis",
            "commissions",
            "trade",
            "sell",
            "buy",
            "cash",
            "reinvestment",
            "balance",
            "estimated",
            "annual",
            "interest",
            "realized",
            "unrealized",
            "gain",
            "loss",
            "net",
            "gross",
            "fee",
            "fees",
            "expense",
            "ratio",
            "return",
            "performance",
            "contribution",
            "distribution",
            "withdrawal",
            "deposit",
            "proceeds",
            "principal",
            "accrued",
            "settlement",
            "transaction",
            "valuation",
            "appreciation",
            "depreciation",
            // Legal / disclaimer terms
            "act",
            "protection",
            "rating",
            "ratings",
            "copyright",
            "intelligence",
            "reproduction",
            "global",
            "reserved",
            "rights",
            "trademark",
            "registered",
            "disclaimer",
            "disclosure",
            "regulation",
            "compliance",
            "provision",
            "liability",
            "warranty",
            "clause",
            // OCR / page structure words
            "page",
            "break",
            // Postal terms
            "po",
            "box",
            // Common abbreviations that trigger ALL CAPS
            "etf",
            "llc",
            "inc",
            "cl",
            "ira",
            "sep",
            "roth",
        ])
    }
}

impl Default for DictionaryDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl NerDetector for DictionaryDetector {
    fn detect(&self, text: &str) -> Vec<Detection> {
        self.detect_names(text)
    }

    fn name(&self) -> &'static str {
        "Dictionary-based NER detector"
    }

    fn is_ready(&self) -> bool {
        true
    }
}

/// Information about a single word token in a line.
struct WordInfo<'a> {
    text: &'a str,
    byte_offset: usize,
    is_sentence_start: bool,
}

/// Iterator over lines in text, yielding (byte offset, line content) pairs.
struct LineIterator<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> LineIterator<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }
}

impl<'a> Iterator for LineIterator<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.text.len() {
            return None;
        }

        let start = self.position;
        let remaining = &self.text[start..];

        let line_end = remaining.find('\n').map_or(remaining.len(), |i| i);
        let line = &remaining[..line_end];

        self.position = start + line_end + 1; // skip past the newline

        Some((start, line))
    }
}

/// Check if a word starts with an uppercase letter.
fn is_capitalized(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|c| c.is_uppercase() && word.len() > 1)
}

/// Check if a word is entirely uppercase letters (at least 2 chars).
fn is_all_caps(word: &str) -> bool {
    word.len() >= 2 && word.chars().all(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_person_name_mid_sentence() {
        let detector = DictionaryDetector::new();
        let text = "Please contact John Smith regarding the matter.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON")
            .collect();
        assert_eq!(person_detections.len(), 1);
        assert_eq!(person_detections[0].matched_text, "John Smith");
        assert_eq!(person_detections[0].rule_name, "NER (dictionary)");
    }

    #[test]
    fn test_does_not_detect_at_sentence_start() {
        let detector = DictionaryDetector::new();
        // "Robert Johnson" starts at the beginning of the sentence (after a period),
        // so the first word should be skipped and it should not be detected.
        let text = "End of sentence. Robert Johnson walked to the store.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON" && d.matched_text == "Robert Johnson")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Should not detect names at sentence start"
        );
    }

    #[test]
    fn test_does_not_detect_common_words() {
        let detector = DictionaryDetector::new();
        // "The" and "Direct" are common words; this should not trigger
        let text = "We received a Direct Deposit from the bank.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON" && d.matched_text == "Direct Deposit")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Should not detect common word sequences"
        );
    }

    #[test]
    fn test_detects_name_after_period() {
        let detector = DictionaryDetector::new();
        let text = "End of sentence. Then John Smith appeared.";
        let detections = detector.detect(text);

        // "Then" is after a period (sentence start), so "Then" is skipped.
        // "John Smith" should be detected since "John" is not at sentence start.
        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON" && d.matched_text == "John Smith")
            .collect();
        assert_eq!(person_detections.len(), 1);
    }

    #[test]
    fn test_detects_three_word_name() {
        let detector = DictionaryDetector::new();
        let text = "We met with Mary Jane Watson at the office.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON")
            .collect();
        assert_eq!(person_detections.len(), 1);
        assert_eq!(person_detections[0].matched_text, "Mary Jane Watson");
    }

    #[test]
    fn test_detects_address_pattern() {
        let detector = DictionaryDetector::new();
        let text = "She lives at 123 Oak Lane in the suburbs.";
        let detections = detector.detect(text);

        let address_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "ADDRESS")
            .collect();
        assert_eq!(address_detections.len(), 1);
        assert_eq!(address_detections[0].matched_text, "123 Oak Lane");
    }

    #[test]
    fn test_detection_positions_are_correct() {
        let detector = DictionaryDetector::new();
        let text = "Please contact John Smith today.";
        let detections = detector.detect(text);

        let d = detections
            .iter()
            .find(|d| d.matched_text == "John Smith")
            .expect("should detect John Smith");
        assert_eq!(&text[d.start..d.end], "John Smith");
    }

    #[test]
    fn test_empty_text() {
        let detector = DictionaryDetector::new();
        let detections = detector.detect("");
        assert!(detections.is_empty());
    }

    #[test]
    fn test_single_capitalized_word_not_detected() {
        let detector = DictionaryDetector::new();
        // A single capitalized word mid-sentence should NOT trigger (need 2+)
        let text = "I spoke with John about the project.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Single capitalized word should not be detected as a name"
        );
    }

    #[test]
    fn test_is_ready() {
        let detector = DictionaryDetector::new();
        assert!(detector.is_ready());
    }

    #[test]
    fn test_name_returns_description() {
        let detector = DictionaryDetector::new();
        assert!(!detector.name().is_empty());
    }

    #[test]
    fn test_detects_all_caps_name() {
        let detector = DictionaryDetector::new();
        let text = "Statement for ROBERT JOHNSON dated today.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON" && d.matched_text == "ROBERT JOHNSON")
            .collect();
        assert_eq!(person_detections.len(), 1);
    }

    #[test]
    fn test_does_not_detect_organization_names() {
        let detector = DictionaryDetector::new();
        let text = "Contact Vanguard Brokerage Services for help.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Should not detect organization names as PERSON"
        );
    }

    #[test]
    fn test_does_not_detect_all_caps_organization() {
        let detector = DictionaryDetector::new();
        let text = "Member of FINRA SIPC and covered.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Should not detect financial organizations as PERSON"
        );
    }

    #[test]
    fn test_does_not_detect_personal_investor_as_name() {
        let detector = DictionaryDetector::new();
        let text = "Your Personal Investor account is ready.";
        let detections = detector.detect(text);

        let person_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.entity_type == "PERSON" && d.matched_text == "Personal Investor")
            .collect();
        assert!(
            person_detections.is_empty(),
            "Should not detect 'Personal Investor' as PERSON"
        );
    }
}
