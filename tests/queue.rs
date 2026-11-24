use niimbot_d101::{
    bitmap::Bitmap,
    engine::Engine,
    ipp::{Request, Response, handle},
};
use std::path::PathBuf;
fn directory(name: &str) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".test-data")
        .join(format!("{name}-{timestamp}"))
}
fn page() -> Bitmap {
    Bitmap {
        width: 8,
        height: 2,
        bits: vec![128, 0],
    }
}

#[tokio::test]
async fn calibration_lowers_readable_output_two_dots_without_accumulating_on_copies() {
    use image::{
        GrayImage, Luma,
        imageops::{rotate90, rotate270},
    };
    let engine = Engine::open(directory("registration"), true, 18631).unwrap();
    let mut cfg = engine.config();
    cfg.address = "01:23:45:67:89:ab".into();
    cfg.set_vertical_offset(2);
    engine.set_config(cfg.clone()).unwrap();
    let art = GrayImage::from_fn(40, 12, |x, y| {
        Luma([if (x, y) == (9, 5) { 0 } else { 255 }])
    });
    let page = Bitmap::from_gray(&rotate270(&art)).unwrap();
    let id = engine.create_job("offset".into(), 2, None).unwrap();
    cfg.address = "another-device".into();
    assert_eq!(cfg.vertical_offset(), 0);
    engine.set_config(cfg).unwrap();
    engine.supply_document(id, vec![page]).unwrap();
    let worker = tokio::spawn(engine.clone().worker());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while engine.job(id).unwrap().state < 7 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(engine.job(id).unwrap().state, 9);
    let dir = engine.directory.join("simulated").join(id.to_string());
    let expected = GrayImage::from_fn(40, 12, |x, y| {
        Luma([if (x, y) == (9, 7) { 0 } else { 255 }])
    });
    for output in ["0001.png", "0002.png"] {
        assert_eq!(
            rotate90(&image::open(dir.join(output)).unwrap().to_luma8()),
            expected
        );
    }
    worker.abort();
}

#[tokio::test]
async fn shutdown_wakes_idle_tasks_and_refuses_new_jobs() {
    let engine = Engine::open(directory("shutdown-idle"), true, 18631).unwrap();
    let worker = tokio::spawn(engine.clone().worker());
    let monitor = tokio::spawn(engine.clone().monitor_printer());
    engine.begin_shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        worker.await.unwrap();
        monitor.await.unwrap();
    })
    .await
    .unwrap();
    engine.disconnect_printer().await;
    assert!(
        engine
            .create_job("late".into(), 1, Some(vec![page()]))
            .is_err()
    );
}

#[tokio::test]
async fn shutdown_cancels_active_batch_without_restarting_it() {
    let engine = Engine::open(directory("shutdown-active"), true, 18631).unwrap();
    let id = engine
        .create_job("active".into(), 10, Some(vec![page()]))
        .unwrap();
    let worker = tokio::spawn(engine.clone().worker());
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while engine.job(id).unwrap().completed == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    engine.begin_shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
    let job = engine.job(id).unwrap();
    assert_eq!(job.state, 7);
    assert!(job.completed < job.total);
}

#[test]
fn second_instance_cannot_touch_active_queue() {
    let path = directory("lock");
    let first = Engine::open(path.clone(), true, 18631).unwrap();
    let id = first
        .create_job("pending".into(), 1, Some(vec![page()]))
        .unwrap();
    assert!(Engine::open(path.clone(), true, 18632).is_err());
    assert_eq!(first.job(id).unwrap().state, 3);
    let disk: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path.join("jobs").join(format!("{id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(disk["state"], 3);
}
#[tokio::test]
async fn six_copies_finish_and_emit_six_files() {
    let engine = Engine::open(directory("copies"), true, 18631).unwrap();
    let id = engine
        .create_job("test".into(), 3, Some(vec![page(), page()]))
        .unwrap();
    let worker = tokio::spawn(engine.clone().worker());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while engine.job(id).unwrap().state < 7 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let job = engine.job(id).unwrap();
    assert_eq!(job.state, 9);
    assert_eq!(job.completed, 6);
    assert_eq!(
        std::fs::read_dir(engine.directory.join("simulated").join(id.to_string()))
            .unwrap()
            .count(),
        6
    );
    worker.abort();
}
#[test]
fn cancel_and_restart_never_reprint_uncertain_pages() {
    let path = directory("restart");
    let engine = Engine::open(path.clone(), true, 18631).unwrap();
    let canceled = engine
        .create_job("cancel".into(), 1, Some(vec![page()]))
        .unwrap();
    engine.cancel(canceled).unwrap();
    assert_eq!(engine.job(canceled).unwrap().state, 7);
    let pending = engine
        .create_job("pending".into(), 1, Some(vec![page()]))
        .unwrap();
    drop(engine);
    let engine = Engine::open(path, true, 18631).unwrap();
    assert_eq!(engine.job(pending).unwrap().state, 8);
    assert_eq!(engine.job(canceled).unwrap().state, 7);
}
#[test]
fn rejects_oversized_jobs_and_duplicate_document() {
    let engine = Engine::open(directory("limits"), true, 18631).unwrap();
    assert!(
        engine
            .create_job("bad".into(), 1000, Some(vec![page(), page()]))
            .is_err()
    );
    let id = engine.create_job("upload".into(), 2, None).unwrap();
    engine.supply_document(id, vec![page()]).unwrap();
    assert!(engine.supply_document(id, vec![page()]).is_err());
}
#[test]
fn capabilities_are_valid_ipp_and_reject_pdf() {
    let engine = Engine::open(directory("ipp"), true, 18631).unwrap();
    let req = Request {
        version: [2, 0],
        operation: 11,
        id: 7,
        attributes: Vec::new(),
        document: Vec::new(),
    };
    let req = Request::parse(&Response::new(&req, 11).finish()).unwrap();
    let response = Request::parse(&handle(&req, &engine)).unwrap();
    assert_eq!(response.operation, 0);
    assert_eq!(response.id, 7);
    assert_eq!(response.text("printer-name"), Some("Niimbot D101"));
    let sizes: Vec<_> = response
        .attributes
        .iter()
        .filter(|a| a.name == "media-supported")
        .map(|a| String::from_utf8(a.value.clone()).unwrap())
        .collect();
    assert_eq!(
        sizes.len(),
        niimbot_d101::label_size::D101LabelSize::ALL.len()
    );
    assert!(sizes.contains(&"custom_d101_25x50mm".into()));
    assert!(!sizes.contains(&"custom_d101_24x40mm".into()));
    assert!(
        response
            .attributes
            .iter()
            .any(|a| a.name == "media-col-database" && a.tag == 0x34)
    );
    let mut invalid = Response::new(&req, 4);
    invalid.text(0x49, "document-format", "application/pdf");
    let invalid = Request::parse(&invalid.finish()).unwrap();
    assert_eq!(
        Request::parse(&handle(&invalid, &engine))
            .unwrap()
            .operation,
        0x040a
    );
}
