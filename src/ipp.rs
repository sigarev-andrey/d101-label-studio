//! Local IPP 2.0 endpoint for the inbox Microsoft IPP Class Driver.
use crate::{
    engine::{Engine, Job},
    label_size::D101LabelSize,
    raster,
};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, ensure};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{StatusCode, header},
    routing::{get, post},
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Attribute {
    pub group: u8,
    pub tag: u8,
    pub name: String,
    pub value: Vec<u8>,
}
pub struct Request {
    pub version: [u8; 2],
    pub operation: u16,
    pub id: i32,
    pub attributes: Vec<Attribute>,
    pub document: Vec<u8>,
}

fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    ensure!(bytes.len() >= n, tr("Обрезанный IPP-пакет"));
    let (a, b) = bytes.split_at(n);
    *bytes = b;
    Ok(a)
}
fn short(bytes: &mut &[u8]) -> Result<usize> {
    Ok(u16::from_be_bytes(take(bytes, 2)?.try_into()?) as usize)
}
impl Request {
    pub fn parse(mut bytes: &[u8]) -> Result<Self> {
        let h = take(&mut bytes, 8)?;
        let version = [h[0], h[1]];
        let operation = u16::from_be_bytes([h[2], h[3]]);
        let id = i32::from_be_bytes(h[4..8].try_into()?);
        let mut attributes = Vec::new();
        let mut group = 0;
        let mut last_name = String::new();
        let mut depth = 0;
        let mut attribute_bytes = 0;
        loop {
            let tag = take(&mut bytes, 1)?[0];
            if tag == 3 {
                ensure!(depth == 0, tr("Незакрытая коллекция IPP"));
                break;
            }
            if tag < 0x10 {
                ensure!(
                    matches!(tag, 1 | 2 | 4 | 5) && depth == 0,
                    tr("Неверная группа IPP")
                );
                group = tag;
                last_name.clear();
                continue;
            }
            ensure!(
                group != 0 && attributes.len() < 4096,
                tr("Слишком много атрибутов IPP")
            );
            let n = short(&mut bytes)?;
            let name = String::from_utf8(take(&mut bytes, n)?.to_vec())?;
            let len = short(&mut bytes)?;
            attribute_bytes += n + len + 5;
            ensure!(attribute_bytes <= 65536, tr("Атрибуты IPP превышают 64 КБ"));
            let value = take(&mut bytes, len)?.to_vec();
            if tag == 0x34 {
                depth += 1;
                ensure!(depth <= 8, tr("Слишком глубокая коллекция IPP"));
            }
            if tag == 0x37 {
                ensure!(depth > 0, tr("Неверное завершение коллекции IPP"));
                depth -= 1;
            }
            let name = if name.is_empty() && depth == 0 && tag != 0x37 {
                last_name.clone()
            } else {
                name
            };
            if !name.is_empty() {
                last_name = name.clone();
            }
            attributes.push(Attribute {
                group,
                tag,
                name,
                value,
            });
        }
        Ok(Self {
            version,
            operation,
            id,
            attributes,
            document: bytes.to_vec(),
        })
    }
    pub fn get(&self, name: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.name == name)
    }
    pub fn text(&self, name: &str) -> Option<&str> {
        std::str::from_utf8(&self.get(name)?.value).ok()
    }
    pub fn integer(&self, name: &str) -> Option<i32> {
        Some(i32::from_be_bytes(
            self.get(name)?.value.as_slice().try_into().ok()?,
        ))
    }
    fn job_id(&self) -> Option<i32> {
        self.integer("job-id")
            .or_else(|| self.text("job-uri")?.rsplit('/').next()?.parse().ok())
    }
}

pub struct Response {
    bytes: Vec<u8>,
}
impl Response {
    pub fn new(request: &Request, status: u16) -> Self {
        let mut bytes = vec![request.version[0], request.version[1]];
        bytes.extend_from_slice(&status.to_be_bytes());
        bytes.extend_from_slice(&request.id.to_be_bytes());
        let mut out = Self { bytes };
        out.group(1);
        out.text(0x47, "attributes-charset", "utf-8");
        out.text(0x48, "attributes-natural-language", "en");
        out
    }
    pub fn group(&mut self, tag: u8) {
        self.bytes.push(tag);
    }
    pub fn attr(&mut self, tag: u8, name: &str, value: &[u8]) {
        self.bytes.push(tag);
        self.bytes
            .extend_from_slice(&(name.len() as u16).to_be_bytes());
        self.bytes.extend_from_slice(name.as_bytes());
        self.bytes
            .extend_from_slice(&(value.len() as u16).to_be_bytes());
        self.bytes.extend_from_slice(value);
    }
    pub fn text(&mut self, tag: u8, name: &str, value: &str) {
        self.attr(tag, name, value.as_bytes());
    }
    pub fn int(&mut self, tag: u8, name: &str, value: i32) {
        self.attr(tag, name, &value.to_be_bytes());
    }
    pub fn boolean(&mut self, name: &str, value: bool) {
        self.attr(0x22, name, &[value as u8]);
    }
    fn strings(&mut self, tag: u8, name: &str, values: &[&str]) {
        for (i, v) in values.iter().enumerate() {
            self.text(tag, if i == 0 { name } else { "" }, v);
        }
    }
    fn member(&mut self, name: &str) {
        self.text(0x4a, "", name);
    }
    fn media_collection(&mut self, name: &str, width: i32, height: i32) {
        self.attr(0x34, name, &[]);
        self.member("media-size");
        self.attr(0x34, "", &[]);
        self.member("x-dimension");
        self.int(0x21, "", width);
        self.member("y-dimension");
        self.int(0x21, "", height);
        self.attr(0x37, "", &[]);
        for side in [
            "media-bottom-margin",
            "media-left-margin",
            "media-right-margin",
            "media-top-margin",
        ] {
            self.member(side);
            self.int(
                0x21,
                "",
                if matches!(side, "media-left-margin" | "media-right-margin") {
                    ((width - 2400) / 2).max(0)
                } else {
                    0
                },
            );
        }
        self.member("media-type");
        self.text(0x44, "", "labels");
        self.member("media-source");
        self.text(0x44, "", "main-roll");
        self.attr(0x37, "", &[]);
    }
    pub fn finish(mut self) -> Vec<u8> {
        self.bytes.push(3);
        self.bytes
    }
}

fn error(req: &Request, code: u16, message: &str) -> Vec<u8> {
    let mut r = Response::new(req, code);
    r.text(0x41, "status-message", message);
    r.finish()
}

fn printer_attributes(req: &Request, engine: &Engine) -> Vec<u8> {
    let config = engine.config();
    let mut r = Response::new(req, 0);
    r.group(4);
    let uri = format!("ipp://127.0.0.1:{}/ipp/print", engine.port);
    r.text(0x45, "printer-uri-supported", &uri);
    r.text(0x44, "uri-authentication-supported", "none");
    r.text(0x44, "uri-security-supported", "none");
    r.text(0x42, "printer-name", "Niimbot D101");
    r.text(
        0x41,
        "printer-info",
        if engine.simulated {
            "Niimbot D101 Bluetooth Bridge (SIMULATION)"
        } else {
            "Niimbot D101 Bluetooth Bridge"
        },
    );
    r.text(0x41, "printer-make-and-model", "NIIMBOT D101");
    r.text(0x41, "printer-location", "This computer / Bluetooth");
    r.text(
        0x45,
        "printer-uuid",
        "urn:uuid:dc1b9101-8b00-4b11-9000-000000000101",
    );
    r.text(
        0x41,
        "printer-device-id",
        "MFG:NIIMBOT;MDL:D101;CMD:PWGRaster;CLS:PRINTER;",
    );
    let jobs = engine.jobs();
    r.int(
        0x23,
        "printer-state",
        if jobs.iter().any(|j| j.state == 5) {
            4
        } else {
            3
        },
    );
    r.text(0x44, "printer-state-reasons", "none");
    r.text(0x41, "printer-state-message", &engine.server_status());
    r.boolean("printer-is-accepting-jobs", engine.ready());
    r.int(
        0x21,
        "queued-job-count",
        jobs.iter().filter(|j| j.state < 7).count() as i32,
    );
    r.int(
        0x21,
        "printer-up-time",
        engine.started.elapsed().as_secs().min(i32::MAX as u64) as i32,
    );
    r.int(0x21, "printer-config-change-time", 0);
    r.int(0x21, "printer-state-change-time", 0);
    r.strings(0x44, "ipp-versions-supported", &["1.1", "2.0"]);
    for (i, op) in [2, 4, 5, 6, 8, 9, 10, 11].iter().enumerate() {
        r.int(0x23, if i == 0 { "operations-supported" } else { "" }, *op);
    }
    r.text(0x47, "charset-configured", "utf-8");
    r.text(0x47, "charset-supported", "utf-8");
    r.text(0x48, "natural-language-configured", "en");
    r.strings(0x48, "generated-natural-language-supported", &["en", "ru"]);
    r.text(0x49, "document-format-default", "image/pwg-raster");
    r.strings(
        0x49,
        "document-format-supported",
        &["image/pwg-raster", "application/octet-stream"],
    );
    r.text(0x44, "compression-supported", "none");
    r.strings(
        0x44,
        "pwg-raster-document-type-supported",
        &["black_1", "sgray_8", "srgb_8"],
    );
    r.text(0x44, "pwg-raster-document-sheet-back", "normal");
    let resolution = [
        203i32.to_be_bytes().as_slice(),
        203i32.to_be_bytes().as_slice(),
        &[3],
    ]
    .concat();
    for key in [
        "printer-resolution-default",
        "printer-resolution-supported",
        "pwg-raster-document-resolution-supported",
    ] {
        r.attr(0x32, key, &resolution);
    }
    r.text(0x44, "print-color-mode-default", "monochrome");
    r.text(0x44, "print-color-mode-supported", "monochrome");
    r.boolean("color-supported", false);
    r.text(0x44, "sides-default", "one-sided");
    r.text(0x44, "sides-supported", "one-sided");
    r.int(0x21, "copies-default", 1);
    r.attr(
        0x33,
        "copies-supported",
        &[1i32.to_be_bytes(), 1000i32.to_be_bytes()].concat(),
    );
    r.int(0x23, "orientation-requested-default", 3);
    for (i, v) in [3, 4, 5, 6].iter().enumerate() {
        r.int(
            0x23,
            if i == 0 {
                "orientation-requested-supported"
            } else {
                ""
            },
            *v,
        );
    }
    r.int(0x23, "print-quality-default", 4);
    r.int(0x23, "print-quality-supported", 4);
    r.int(0x23, "finishings-default", 3);
    r.int(0x23, "finishings-supported", 3);
    r.boolean("multiple-document-jobs-supported", false);
    r.text(
        0x44,
        "multiple-document-handling-default",
        "separate-documents-collated-copies",
    );
    r.strings(
        0x44,
        "multiple-document-handling-supported",
        &["single-document", "separate-documents-collated-copies"],
    );
    r.text(0x44, "job-hold-until-default", "no-hold");
    r.text(0x44, "job-hold-until-supported", "no-hold");
    r.text(0x44, "job-sheets-default", "none");
    r.text(0x44, "job-sheets-supported", "none");
    r.boolean("page-ranges-supported", false);
    r.int(0x21, "job-priority-default", 50);
    r.int(0x21, "job-priority-supported", 1);
    r.attr(
        0x33,
        "job-k-octets-supported",
        &[0i32.to_be_bytes(), 32768i32.to_be_bytes()].concat(),
    );
    r.int(0x21, "job-cancel-after-default", 0);
    r.int(0x21, "multiple-operation-time-out", 60);
    r.text(0x44, "multiple-operation-time-out-action", "abort-job");
    r.strings(
        0x44,
        "job-creation-attributes-supported",
        &[
            "copies",
            "media",
            "media-col",
            "document-format",
            "job-name",
            "ipp-attribute-fidelity",
            "orientation-requested",
            "printer-resolution",
            "print-quality",
            "print-color-mode",
            "sides",
            "multiple-document-handling",
        ],
    );
    let width = (config.label.width_mm * 100.0).round() as i32;
    let height = (config.label.length_mm * 100.0).round() as i32;
    let media = format!(
        "custom_d101_{}x{}mm",
        config.label.width_mm, config.label.length_mm
    );
    for key in ["media-default", "media-ready"] {
        r.text(0x44, key, &media);
    }
    for key in ["media-col-default", "media-col-ready"] {
        r.media_collection(key, width, height);
    }
    for (i, size) in D101LabelSize::ALL.iter().enumerate() {
        let (w, l) = size.tenths();
        r.text(
            0x44,
            if i == 0 { "media-supported" } else { "" },
            &format!("custom_d101_{}x{}mm", w as f32 / 10.0, l as f32 / 10.0),
        );
    }
    for (i, size) in D101LabelSize::ALL.iter().enumerate() {
        let (w, l) = size.tenths();
        r.media_collection(
            if i == 0 { "media-col-database" } else { "" },
            i32::from(w) * 10,
            i32::from(l) * 10,
        );
    }
    r.strings(
        0x44,
        "media-col-supported",
        &[
            "media-size",
            "media-bottom-margin",
            "media-left-margin",
            "media-right-margin",
            "media-top-margin",
            "media-source",
            "media-type",
        ],
    );
    for (i, size) in D101LabelSize::ALL.iter().enumerate() {
        let (w, l) = size.tenths();
        r.attr(0x34, if i == 0 { "media-size-supported" } else { "" }, &[]);
        r.member("x-dimension");
        r.int(0x21, "", i32::from(w) * 10);
        r.member("y-dimension");
        r.int(0x21, "", i32::from(l) * 10);
        r.attr(0x37, "", &[]);
    }
    for side in [
        "media-bottom-margin-supported",
        "media-left-margin-supported",
        "media-right-margin-supported",
        "media-top-margin-supported",
    ] {
        r.int(0x21, side, 0);
        if matches!(
            side,
            "media-left-margin-supported" | "media-right-margin-supported"
        ) {
            r.int(0x21, "", 50);
        }
    }
    r.text(0x44, "media-type-supported", "labels");
    r.text(0x44, "media-source-supported", "main-roll");
    r.finish()
}

fn job_attributes(r: &mut Response, job: &Job, port: u16) {
    r.group(2);
    r.int(0x21, "job-id", job.id);
    r.text(
        0x45,
        "job-uri",
        &format!("ipp://127.0.0.1:{port}/ipp/print/jobs/{}", job.id),
    );
    r.text(
        0x45,
        "job-printer-uri",
        &format!("ipp://127.0.0.1:{port}/ipp/print"),
    );
    r.text(0x42, "job-name", &job.name);
    r.int(0x23, "job-state", job.state);
    r.text(
        0x44,
        "job-state-reasons",
        match job.state {
            4 => "job-incoming",
            5 => "job-printing",
            7 => "job-canceled-by-user",
            8 => "aborted-by-system",
            9 => "job-completed-successfully",
            _ => "none",
        },
    );
    r.text(0x41, "job-state-message", &job.message);
    r.int(0x21, "job-impressions", job.total as i32);
    r.int(0x21, "job-impressions-completed", job.completed as i32);
    r.int(0x21, "job-media-sheets-completed", job.completed as i32);
    r.int(0x21, "copies", job.copies as i32);
}

fn validate(req: &Request) -> Result<()> {
    if let Some(a) = req.get("copies") {
        ensure!(
            a.tag == 0x21
                && req
                    .integer("copies")
                    .is_some_and(|n| (1..=1000).contains(&n)),
            "copies must be 1..1000"
        );
    }
    ensure!(
        matches!(
            req.text("document-format"),
            None | Some("image/pwg-raster" | "application/octet-stream")
        ),
        "Only PWG Raster is supported"
    );
    ensure!(
        matches!(req.text("compression"), None | Some("none")),
        "Compression is not supported"
    );
    ensure!(
        matches!(req.text("sides"), None | Some("one-sided")),
        "Only one-sided printing is supported"
    );
    ensure!(
        matches!(req.text("job-hold-until"), None | Some("no-hold")),
        "Held jobs are not supported"
    );
    ensure!(
        matches!(
            req.text("multiple-document-handling"),
            None | Some("single-document" | "separate-documents-collated-copies")
        ),
        "Only collated copies are supported"
    );
    Ok(())
}

pub fn handle(req: &Request, engine: &Engine) -> Vec<u8> {
    if !matches!(req.version, [1, 1] | [2, 0]) {
        return error(req, 0x0503, "IPP version not supported");
    }
    if req.text("attributes-charset") != Some("utf-8") {
        return error(req, 0x040d, "UTF-8 is required");
    }
    if matches!(req.operation, 2 | 4 | 5 | 6)
        && let Err(e) = validate(req)
    {
        return error(
            req,
            if e.to_string().contains("PWG") {
                0x040a
            } else {
                0x040b
            },
            &e.to_string(),
        );
    }
    match req.operation {
        11 => printer_attributes(req, engine),
        4 => Response::new(req, 0).finish(),
        2 | 5 => {
            let pages = if req.operation == 2 {
                match raster::decode(&req.document) {
                    Ok(p) => Some(p),
                    Err(e) => return error(req, 0x0411, &e.to_string()),
                }
            } else {
                None
            };
            match engine.create_job(
                req.text("job-name")
                    .unwrap_or("Windows print job")
                    .to_string(),
                req.integer("copies").unwrap_or(1) as usize,
                pages,
            ) {
                Ok(id) => {
                    let mut r = Response::new(req, 0);
                    job_attributes(&mut r, &engine.job(id).unwrap(), engine.port);
                    r.finish()
                }
                Err(e) => error(req, 0x0507, &e.to_string()),
            }
        }
        6 => {
            let Some(id) = req.job_id() else {
                return error(req, 0x0400, "Missing job-id");
            };
            if engine.job(id).is_none() {
                return error(req, 0x0406, "Job not found");
            }
            if req.get("last-document").map(|a| a.value.as_slice()) != Some(&[1][..]) {
                return error(
                    req,
                    0x0509,
                    "Send a single document with last-document=true",
                );
            }
            let pages = match raster::decode(&req.document) {
                Ok(p) => p,
                Err(e) => return error(req, 0x0411, &e.to_string()),
            };
            match engine.supply_document(id, pages) {
                Ok(()) => {
                    let mut r = Response::new(req, 0);
                    job_attributes(&mut r, &engine.job(id).unwrap(), engine.port);
                    r.finish()
                }
                Err(e) => error(req, 0x0404, &e.to_string()),
            }
        }
        8 | 9 => {
            let Some(id) = req.job_id() else {
                return error(req, 0x0400, "Missing job-id");
            };
            let Some(job) = engine.job(id) else {
                return error(req, 0x0406, "Job not found");
            };
            if req.operation == 8 {
                match engine.cancel(id) {
                    Ok(()) => Response::new(req, 0).finish(),
                    Err(e) => error(req, 0x0404, &e.to_string()),
                }
            } else {
                let mut r = Response::new(req, 0);
                job_attributes(&mut r, &job, engine.port);
                r.finish()
            }
        }
        10 => {
            let mut r = Response::new(req, 0);
            let which = req.text("which-jobs").unwrap_or("not-completed");
            let limit = req.integer("limit").unwrap_or(100).clamp(1, 100) as usize;
            for job in engine
                .jobs()
                .iter()
                .filter(|j| match which {
                    "completed" => j.state >= 7,
                    "all" => true,
                    _ => j.state < 7,
                })
                .take(limit)
            {
                job_attributes(&mut r, job, engine.port);
            }
            r.finish()
        }
        _ => error(req, 0x0501, "Operation not supported"),
    }
}

async fn endpoint(
    State(engine): State<Arc<Engine>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], Vec<u8>) {
    let mime = [(header::CONTENT_TYPE, "application/ipp")];
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").trim())
        != Some("application/ipp")
    {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, mime, Vec::new());
    }
    match Request::parse(&body) {
        Ok(req) => {
            let bytes = handle(&req, &engine);
            (StatusCode::OK, mime, bytes)
        }
        Err(_) => (StatusCode::BAD_REQUEST, mime, Vec::new()),
    }
}

pub async fn serve(engine: Arc<Engine>) -> Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, engine.port))
        .await
        .context(tr(
            "Не удалось открыть IPP-порт: возможно, приложение уже запущено",
        ))?;
    serve_listener(engine, listener).await
}

pub async fn serve_listener(engine: Arc<Engine>, listener: tokio::net::TcpListener) -> Result<()> {
    let app = Router::new()
        .route("/ipp/print", post(endpoint))
        .route("/", post(endpoint))
        .route("/health", get(|| async { "Niimbot D101 IPP bridge" }))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(engine.clone());
    engine.set_server_status(local_format!(
        "IPP готов: 127.0.0.1:{}",
        "IPP ready: 127.0.0.1:{}",
        engine.port
    ));
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_attributes_are_rejected() {
        let req = Request {
            version: [2, 0],
            operation: 11,
            id: 15,
            attributes: Vec::new(),
            document: Vec::new(),
        };
        let bytes = Response::new(&req, 11).finish();
        for n in 0..bytes.len() {
            assert!(Request::parse(&bytes[..n]).is_err());
        }
        assert_eq!(Request::parse(&bytes).unwrap().id, 15);
    }
    #[test]
    fn collections_and_repeated_values() {
        let req = Request {
            version: [2, 0],
            operation: 11,
            id: 1,
            attributes: Vec::new(),
            document: Vec::new(),
        };
        let mut r = Response::new(&req, 0);
        r.group(4);
        r.media_collection("media-col-default", 1400, 4000);
        r.strings(
            0x44,
            "sides-supported",
            &["one-sided", "two-sided-long-edge"],
        );
        let decoded = Request::parse(&r.finish()).unwrap();
        assert_eq!(
            decoded
                .attributes
                .iter()
                .filter(|a| a.name == "sides-supported")
                .count(),
            2
        );
    }
}
