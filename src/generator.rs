//! Expand text placeholders without changing the template's physical layout.
use crate::document::{Content, Document, Label};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Combine {
    #[default]
    Rows,
    Product,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Range {
        start: i64,
        end: i64,
        step: i64,
        digits: u8,
    },
    List {
        text: String,
    },
}
impl Default for Source {
    fn default() -> Self {
        Self::Range {
            start: 1,
            end: 10,
            step: 1,
            digits: 0,
        }
    }
}
impl Source {
    pub fn values(&self) -> Result<Vec<String>> {
        match self {
            Self::Range {
                start,
                end,
                step,
                digits,
            } => {
                ensure!(*step != 0, tr("Шаг диапазона не может быть нулевым"));
                let distance = *end as i128 - *start as i128;
                ensure!(
                    distance == 0 || distance.signum() == (*step as i128).signum(),
                    tr("Шаг должен вести от начала диапазона к концу")
                );
                let count = distance.abs() / (*step as i128).abs() + 1;
                ensure!(count <= 1000, tr("В диапазоне больше 1000 значений"));
                ensure!(*digits <= 12, tr("Не более 12 цифр с ведущими нулями"));
                Ok((0..count)
                    .map(|i| {
                        let n = *start as i128 + i * *step as i128;
                        let sign = if n < 0 { "-" } else { "" };
                        format!("{sign}{:0width$}", n.abs(), width = *digits as usize)
                    })
                    .collect())
            }
            Self::List { text } => {
                ensure!(text.len() <= 262_144, tr("Список превышает 256 КБ"));
                let values: Vec<_> = text
                    // WinUI TextBox uses bare CR; pasted files also use LF / CRLF.
                    .split(['\r', '\n'])
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_owned)
                    .collect();
                ensure!(
                    !values.is_empty(),
                    tr("Добавьте хотя бы одну непустую строку")
                );
                ensure!(values.len() <= 1000, tr("В списке больше 1000 значений"));
                ensure!(
                    values.iter().all(|v| v.chars().count() <= 4096),
                    tr("Строка списка длиннее 4096 символов")
                );
                Ok(values)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub variables: Vec<Variable>,
    pub combine: Combine,
    pub copies: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            variables: vec![],
            combine: Combine::Rows,
            copies: 1,
        }
    }
}
impl Settings {
    /// Retain sources by name when changing templates; stale variables are not expanded.
    pub fn for_template(&self, label: &Label) -> Result<Self> {
        let variables = variable_names(label)?
            .into_iter()
            .map(|name| {
                self.variables
                    .iter()
                    .find(|v| v.name == name)
                    .cloned()
                    .unwrap_or(Variable {
                        name,
                        source: Source::default(),
                    })
            })
            .collect();
        Ok(Self {
            variables,
            combine: self.combine,
            copies: self.copies,
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.variables.len() <= 32, tr("Не более 32 переменных"));
        ensure!(
            (1..=1000).contains(&self.copies),
            tr("Количество копий: от 1 до 1000")
        );
        let mut seen = BTreeSet::new();
        let mut bytes = 0;
        for v in &self.variables {
            ensure!(
                valid_name(&v.name) && seen.insert(&v.name),
                tr("Некорректное или повторяющееся имя переменной")
            );
            if let Source::List { text } = &v.source {
                bytes += text.len();
            }
        }
        ensure!(bytes <= 1_048_576, tr("Списки генератора превышают 1 МБ"));
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Variable(String),
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
}
fn parse(text: &str) -> Result<Vec<Part>> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' if chars.peek() == Some(&c) => {
                chars.next();
                literal.push(c);
            }
            '{' => {
                if !literal.is_empty() {
                    parts.push(Part::Literal(std::mem::take(&mut literal)));
                }
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                ensure!(
                    closed,
                    local_format!(
                        "Не закрыта переменная: добавьте символ }}",
                        "Unclosed variable. Add }}"
                    )
                );
                ensure!(
                    valid_name(&name),
                    tr(
                        "Имя в фигурных скобках должно начинаться с буквы или _ и содержать только буквы, цифры и _"
                    )
                );
                parts.push(Part::Variable(name));
            }
            '}' => bail!(local_format!(
                "Лишняя закрывающая скобка. Для обычных скобок используйте {{{{ и }}}}",
                "Unexpected closing brace. Use {{{{ and }}}} for literal braces"
            )),
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }
    Ok(parts)
}
pub fn variable_names(label: &Label) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for block in &label.blocks {
        if let Content::Text { text, .. } = &block.content {
            for part in parse(text)? {
                if let Part::Variable(name) = part
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
    }
    ensure!(names.len() <= 32, tr("Не более 32 переменных в шаблоне"));
    Ok(names)
}

pub struct Plan {
    template: Label,
    parts: Vec<Option<Vec<Part>>>,
    values: Vec<(String, Vec<String>)>,
    settings: Settings,
    pub count: usize,
}
impl Plan {
    pub fn new(doc: &Document, page: usize, settings: &Settings) -> Result<Self> {
        doc.validate()?;
        settings.validate()?;
        let template = doc
            .labels
            .get(page)
            .context(tr("Этикетка-шаблон не найдена"))?;
        let names = variable_names(template)?;
        ensure!(
            !names.is_empty(),
            local_format!(
                "Добавьте переменную в текст шаблона, например: Кабель {{номер}}",
                "Add a variable to the template text, for example Cable {{number}}"
            )
        );
        let mut values = Vec::new();
        for name in names {
            let variable = settings
                .variables
                .iter()
                .find(|v| v.name == name)
                .with_context(|| {
                    local_format!("Нет источника для {{{name}}}", "No source for {{{name}}}")
                })?;
            values.push((
                name.clone(),
                variable.source.values().with_context(|| {
                    local_format!("Переменная {{{name}}}", "Variable {{{name}}}")
                })?,
            ));
        }
        let count = match settings.combine {
            Combine::Rows => {
                let count = values.iter().map(|(_, v)| v.len()).max().unwrap();
                ensure!(
                    values.iter().all(|(_, v)| v.len() == 1 || v.len() == count),
                    tr(
                        "При подстановке по строкам списки должны иметь одинаковую длину. Одно значение можно повторить для всех строк."
                    )
                );
                count
            }
            Combine::Product => values.iter().try_fold(1usize, |n, (_, v)| {
                n.checked_mul(v.len())
                    .filter(|n| *n <= 1000)
                    .context(tr("Больше 1000 сочетаний; сократите диапазоны или списки"))
            })?,
        };
        ensure!(
            doc.labels.len() + count <= 1000,
            tr("В проекте может быть не больше 1000 этикеток, включая шаблон")
        );
        let existing_copies: usize = doc
            .labels
            .iter()
            .enumerate()
            .filter(|(i, l)| *i != page && l.enabled)
            .map(|(_, l)| l.copies)
            .sum();
        ensure!(
            existing_copies + count * settings.copies <= 1000,
            tr(
                "В наборе получится больше 1000 экземпляров; уменьшите количество или исключите прежние этикетки из печати"
            )
        );
        let image_bytes = |label: &Label| {
            label
                .blocks
                .iter()
                .map(|b| match &b.content {
                    Content::Image { png, .. } => png.len(),
                    _ => 0,
                })
                .sum::<usize>()
        };
        ensure!(
            doc.labels.iter().map(image_bytes).sum::<usize>() + image_bytes(template) * count
                <= 16 * 1024 * 1024,
            tr(
                "Изображения в создаваемом наборе превысят 16 МБ; уменьшите количество этикеток или размер изображения"
            )
        );
        ensure!(
            doc.next_id()
                .checked_add((template.blocks.len() * count) as u64)
                .is_some_and(|id| id < u64::MAX),
            tr("Исчерпаны идентификаторы блоков")
        );
        let parts = template
            .blocks
            .iter()
            .map(|b| match &b.content {
                Content::Text { text, .. } => parse(text).map(Some),
                _ => Ok(None),
            })
            .collect::<Result<_>>()?;
        let plan = Self {
            template: template.clone(),
            parts,
            values,
            settings: settings.clone(),
            count,
        };
        // Check expansion before cloning any image-heavy labels or modifying the project.
        let mut bytes = 0;
        for row in 0..count {
            for text in plan.texts(row).into_iter().flatten() {
                ensure!(
                    text.chars().count() <= 4096,
                    local_format!(
                        "После подстановки текст длиннее 4096 символов (этикетка {})",
                        "Expanded text exceeds 4,096 characters on label {}",
                        row + 1
                    )
                );
                bytes += text.len();
                ensure!(
                    bytes <= 8 * 1024 * 1024,
                    tr("Текст создаваемого набора превышает 8 МБ")
                );
            }
        }
        // Pretty JSON stores image bytes as indented decimal numbers. Keep the
        // saved project below Document::load's 64-MB limit as well as bounding RAM.
        let base_text: usize = doc
            .labels
            .iter()
            .flat_map(|l| &l.blocks)
            .map(|b| match &b.content {
                Content::Text { text, .. } => text.len(),
                _ => 0,
            })
            .sum();
        let block_count = doc.labels.iter().map(|l| l.blocks.len()).sum::<usize>()
            + template.blocks.len() * count;
        let images =
            doc.labels.iter().map(image_bytes).sum::<usize>() + image_bytes(template) * count;
        let source_bytes: usize = settings
            .variables
            .iter()
            .map(|v| match &v.source {
                Source::List { text } => text.len(),
                _ => 0,
            })
            .sum();
        ensure!(
            images * 24
                + (base_text + bytes + source_bytes) * 6
                + block_count * 2048
                + doc.labels.len() * 1024
                < 64 * 1024 * 1024,
            tr(
                "Проект с таким набором будет слишком большим для сохранения; уменьшите количество этикеток или размер изображений"
            )
        );
        Ok(plan)
    }
    pub fn total_copies(&self) -> usize {
        self.count * self.settings.copies
    }
    fn texts(&self, row: usize) -> Vec<Option<String>> {
        let mut rest = row;
        let mut row_values = BTreeMap::new();
        for (name, values) in self.values.iter().rev() {
            let i = match self.settings.combine {
                Combine::Rows => {
                    if values.len() == 1 {
                        0
                    } else {
                        row
                    }
                }
                Combine::Product => {
                    let i = rest % values.len();
                    rest /= values.len();
                    i
                }
            };
            row_values.insert(name.as_str(), values[i].as_str());
        }
        self.parts
            .iter()
            .map(|parts| {
                parts.as_ref().map(|parts| {
                    parts
                        .iter()
                        .map(|p| match p {
                            Part::Literal(s) => s.as_str(),
                            Part::Variable(name) => row_values[name.as_str()],
                        })
                        .collect()
                })
            })
            .collect()
    }
    pub fn label(&self, row: usize) -> Result<Label> {
        ensure!(row < self.count, tr("Нет такого образца"));
        let mut label = self.template.clone();
        for (block, text) in label.blocks.iter_mut().zip(self.texts(row)) {
            if let Content::Text { text: target, .. } = &mut block.content {
                *target = text.unwrap();
            }
        }
        label.name = format!(
            "{} · {}",
            self.template.name.chars().take(85).collect::<String>(),
            row + 1
        );
        label.enabled = true;
        label.copies = self.settings.copies;
        Ok(label)
    }
}

/// Atomic generation: the source is never mutated, and the complete result is validated.
pub fn generate(doc: &Document, page: usize, settings: &Settings) -> Result<Document> {
    let plan = Plan::new(doc, page, settings)?;
    let mut result = doc.clone();
    result.generator = settings.clone();
    result.labels[page].enabled = false;
    let mut id = result.next_id();
    for row in 0..plan.count {
        let mut label = plan.label(row)?;
        for block in &mut label.blocks {
            block.id = id;
            id += 1;
        }
        result.labels.push(label);
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bitmap::LabelSpec,
        document::{Block, Bounds},
    };

    fn template(text: &str) -> Document {
        let mut doc = Document::new(LabelSpec::default());
        if let Content::Text {
            text: target,
            style,
        } = &mut doc.labels[0].blocks[0].content
        {
            *target = text.into();
            style.bold = true;
            style.vertical_alignment = 2;
        }
        doc
    }
    fn text(label: &Label) -> &str {
        match &label.blocks[0].content {
            Content::Text { text, .. } => text,
            _ => panic!("Not text"),
        }
    }
    fn list(name: &str, value: &str) -> Variable {
        Variable {
            name: name.into(),
            source: Source::List { text: value.into() },
        }
    }
    #[test]
    fn ranges_are_inclusive_bounded_and_support_descending_and_zero_padding() {
        assert_eq!(
            Source::Range {
                start: -2,
                end: 2,
                step: 2,
                digits: 3
            }
            .values()
            .unwrap(),
            ["-002", "000", "002"]
        );
        assert_eq!(
            Source::Range {
                start: 10,
                end: 1,
                step: -3,
                digits: 0
            }
            .values()
            .unwrap(),
            ["10", "7", "4", "1"]
        );
        assert_eq!(
            Source::Range {
                start: 1,
                end: 6,
                step: 2,
                digits: 0
            }
            .values()
            .unwrap(),
            ["1", "3", "5"]
        );
        for source in [
            Source::Range {
                start: 1,
                end: 10,
                step: 0,
                digits: 0,
            },
            Source::Range {
                start: 10,
                end: 1,
                step: 1,
                digits: 0,
            },
            Source::Range {
                start: i64::MIN,
                end: i64::MAX,
                step: 1,
                digits: 0,
            },
            Source::Range {
                start: 1,
                end: 2,
                step: 1,
                digits: 13,
            },
        ] {
            assert!(source.values().is_err());
        }
        assert_eq!(
            Source::Range {
                start: i64::MAX,
                end: i64::MIN,
                step: i64::MIN,
                digits: 0
            }
            .values()
            .unwrap(),
            [i64::MAX.to_string(), "-1".into()]
        );
    }
    #[test]
    fn lists_preserve_unicode_duplicates_and_spaces_but_skip_empty_rows() {
        assert_eq!(
            Source::List {
                text: "\r\nПитание\r\n\r\nLAN\nLAN\n резерв \n".into()
            }
            .values()
            .unwrap(),
            ["Питание", "LAN", "LAN", " резерв "]
        );
        assert_eq!(
            Source::List {
                text: "LAN\rПитание\r\rРезерв\r".into()
            }
            .values()
            .unwrap(),
            ["LAN", "Питание", "Резерв"]
        );
        assert!(
            Source::List {
                text: " \n\r\n".into()
            }
            .values()
            .is_err()
        );
    }
    #[test]
    fn placeholders_repeat_across_blocks_and_escaped_or_substituted_braces_stay_literal() {
        let mut doc = template("{{номер}}: {номер} / {номер}");
        doc.add_text(0, "{имя}".into());
        let settings = Settings {
            variables: vec![list("номер", "{имя}"), list("имя", "LAN")],
            ..Default::default()
        };
        let result = generate(&doc, 0, &settings).unwrap();
        assert_eq!(text(&result.labels[1]), "{номер}: {имя} / {имя}");
        assert!(
            matches!(&result.labels[1].blocks[1].content,Content::Text{text,..} if text=="LAN")
        );
        for bad in [
            "{номер",
            "номер}",
            "{}",
            "{bad name}",
            "{123}",
            "{outer{inner}}",
        ] {
            assert!(variable_names(&template(bad).labels[0]).is_err(), "{bad}");
        }
    }
    #[test]
    fn rows_pair_values_broadcast_constants_and_reject_mismatched_lengths() {
        let doc = template("{номер}: {имя} · {объект}");
        let mut settings = Settings {
            variables: vec![
                list("номер", "1\n2"),
                list("имя", "LAN\nUSB"),
                list("объект", "Шкаф А"),
            ],
            ..Default::default()
        };
        let plan = Plan::new(&doc, 0, &settings).unwrap();
        assert_eq!(plan.count, 2);
        assert_eq!(text(&plan.label(1).unwrap()), "2: USB · Шкаф А");
        settings.variables[1] = list("имя", "LAN\nUSB\nPower");
        assert!(Plan::new(&doc, 0, &settings).is_err());
        settings.variables.pop();
        assert!(Plan::new(&doc, 0, &settings).is_err());
    }
    #[test]
    fn product_has_stable_order_and_preview_matches_generated_labels() {
        let doc = template("{буква}-{номер}");
        let settings = Settings {
            combine: Combine::Product,
            variables: vec![list("буква", "A\nB"), list("номер", "1\n2\n3")],
            copies: 2,
        };
        let plan = Plan::new(&doc, 0, &settings).unwrap();
        assert_eq!(plan.count, 6);
        assert_eq!(plan.total_copies(), 12);
        let generated = generate(&doc, 0, &settings).unwrap();
        let expected = ["A-1", "A-2", "A-3", "B-1", "B-2", "B-3"];
        for (i, value) in expected.into_iter().enumerate() {
            assert_eq!(text(&plan.label(i).unwrap()), value);
            assert_eq!(text(&generated.labels[i + 1]), value);
        }
        assert!(plan.label(6).is_err());
        assert_eq!(generated.print_pages().unwrap().len(), 12);
    }
    #[test]
    fn generation_is_atomic_preserves_all_block_properties_and_allocates_unique_ids() {
        let mut doc = template("Кабель {номер}");
        doc.labels[0].blocks.push(Block {
            id: 40,
            bounds: Bounds {
                x: 1.0,
                y: 1.0,
                width: 10.0,
                height: 3.0,
            },
            content: Content::Rectangle {
                stroke_mm: 0.2,
                filled: false,
            },
        });
        let original = doc.clone();
        let settings = doc.generator.for_template(&doc.labels[0]).unwrap();
        let result = generate(&doc, 0, &settings).unwrap();
        assert_eq!(doc, original);
        assert_eq!(result.labels.len(), 11);
        assert!(!result.labels[0].enabled);
        assert_eq!(text(&result.labels[0]), "Кабель {номер}");
        for label in &result.labels[1..] {
            assert!(label.enabled);
            assert_eq!(label.blocks[0].bounds, doc.labels[0].blocks[0].bounds);
            if let (Content::Text { style: a, .. }, Content::Text { style: b, .. }) =
                (&label.blocks[0].content, &doc.labels[0].blocks[0].content)
            {
                assert_eq!(a, b);
            }
            assert_eq!(label.blocks[1].content, doc.labels[0].blocks[1].content);
            assert!(label.blocks.iter().all(|b| b.id > 40));
        }
        result.validate().unwrap(); // Includes global block-ID uniqueness.
    }
    #[test]
    fn expansion_and_batch_limits_are_checked_before_modifying_the_document() {
        let doc = template("{а}{а}");
        let mut settings = Settings {
            variables: vec![list("а", &"я".repeat(3000))],
            ..Default::default()
        };
        assert!(generate(&doc, 0, &settings).is_err());
        settings.variables[0].source = Source::default();
        settings.copies = 101;
        assert!(generate(&doc, 0, &settings).is_err());
        settings.copies = 1;
        settings.variables[0].source = Source::Range {
            start: 1,
            end: 1000,
            step: 1,
            digits: 0,
        };
        assert!(generate(&doc, 0, &settings).is_err()); // Plus the retained template.
        let mut with_image = doc.clone();
        let bounds = with_image.labels[0].blocks[0].bounds;
        with_image.labels[0].blocks.push(Block {
            id: 2,
            bounds,
            content: Content::Image {
                name: "large".into(),
                png: vec![0; 2 * 1024 * 1024],
                threshold: 128,
            },
        });
        settings.variables[0].source = Source::default();
        assert!(Plan::new(&with_image, 0, &settings).is_err());
        assert_eq!(text(&doc.labels[0]), "{а}{а}");
    }
    #[test]
    fn settings_round_trip_and_old_projects_without_settings_still_load() {
        let mut doc = template("{номер}");
        doc.generator = doc.generator.for_template(&doc.labels[0]).unwrap();
        let json = serde_json::to_vec(&doc).unwrap();
        assert_eq!(serde_json::from_slice::<Document>(&json).unwrap(), doc);
        let mut old = serde_json::to_value(&doc).unwrap();
        old.as_object_mut().unwrap().remove("generator");
        let loaded: Document = serde_json::from_value(old).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.generator, Settings::default());
        let mut changed = template("{имя} {номер}");
        changed.generator = doc.generator.for_template(&changed.labels[0]).unwrap();
        assert_eq!(changed.generator.variables[1], doc.generator.variables[0]);
    }
}
