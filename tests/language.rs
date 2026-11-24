use niimbot_d101::{
    bitmap::LabelSpec,
    document::{Content, Document},
    engine::Engine,
    i18n::{Language, tr},
    label_size::D101LabelSize,
};

#[test]
fn english_survives_restart_and_never_translates_project_text() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("d101-language-{stamp}"));
    let engine = Engine::open(dir.clone(), true, 0).unwrap();
    assert_eq!(engine.config().language, Language::Ru);
    let mut doc = Document::new(LabelSpec::default());
    doc.add_text(0, "Печать".into());
    engine.set_language(Language::En).unwrap();
    assert_eq!(tr("Печать"), "Print");
    assert_eq!(D101LabelSize::W12L40.title(), "12 × 40 mm");
    assert_eq!(engine.printer_status().battery_text(), "No data");
    assert!(
        matches!(&doc.labels[0].blocks[1].content, Content::Text {text, ..} if text == "Печать")
    );
    drop(engine);
    Language::Ru.activate();
    let engine = Engine::open(dir.clone(), true, 0).unwrap();
    assert_eq!(Language::current(), Language::En);
    engine.set_language(Language::Ru).unwrap();
    assert_eq!(tr("Печать"), "Печать");
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();
}
