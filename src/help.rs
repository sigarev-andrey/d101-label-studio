//! The same manuals are embedded in the editor and distributed as Markdown.
use crate::i18n::Language;

pub struct Topic<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

pub fn search(language: Language, query: &str) -> Vec<Topic<'static>> {
    let manual = match language {
        Language::Ru => include_str!("../docs/guide-ru.md"),
        Language::En => include_str!("../docs/guide-en.md"),
    };
    let normalize = |s: &str| s.to_lowercase().replace('ё', "е");
    let normalized = normalize(query);
    let terms: Vec<_> = normalized.split_whitespace().collect();
    let mut matches = Vec::new();
    for section in manual.split("\n## ").skip(1) {
        let Some((title, body)) = section.split_once('\n') else {
            continue;
        };
        let searchable = normalize(section);
        if terms.iter().all(|term| searchable.contains(term)) {
            matches.push(Topic {
                title: title.trim(),
                body: body.trim(),
            });
        }
    }
    matches.sort_by_key(|t| !terms.iter().all(|term| normalize(t.title).contains(term)));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_manuals_support_browsing_and_full_text_search() {
        for (lang, query) in [(Language::Ru, "RFID расход"), (Language::En, "RFID usage")] {
            let all = search(lang, "");
            assert!(all.len() >= 10);
            assert!(all.iter().all(|t| !t.body.is_empty()));
            assert!(!search(lang, query).is_empty());
            assert!(search(lang, "zzzz-unmatched").is_empty());
        }
    }
}
