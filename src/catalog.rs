//! Product metadata only. Consumption always comes from the current printer RFID reply.
use crate::label_outline::LabelOutline;
use crate::label_size::D101LabelSize;
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollMetadata {
    pub barcode: String,
    pub name: String,
    pub size: Option<D101LabelSize>,
    pub capacity: Option<u16>,
    #[serde(default)]
    pub outline: Option<LabelOutline>,
}
fn valid_code(code: &str) -> bool {
    (4..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric())
}
pub fn parse_metadata(bytes: &[u8], code: &str) -> Result<RollMetadata> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(
        value["code"].as_i64() == Some(1),
        tr("Каталог не нашёл этот расходник")
    );
    let data = &value["data"];
    ensure!(
        data["barcode"].as_str() == Some(code)
            || data["profile"]["extrain"]["virtualBarCode"].as_str() == Some(code),
        tr("Каталог вернул другой код рулона")
    );
    ensure!(
        data["profile"]["machineName"]
            .as_str()
            .is_some_and(|s| s.split(',').any(|m| m.trim() == "D101")),
        tr("Каталог не подтверждает совместимость с D101")
    );
    let width = data["width"].as_f64().context(tr("Нет ширины шаблона"))?;
    let height = data["height"].as_f64().context(tr("Нет высоты шаблона"))?;
    ensure!(
        width > 0.0 && width <= 200.0 && height > 0.0 && height <= 200.0,
        tr("Некорректный размер каталога")
    );
    let rotation = data["rotate"]
        .as_i64()
        .context(tr("Нет ориентации шаблона"))?;
    let (across, mut feed) = match rotation.rem_euclid(360) {
        0 | 180 => (width, height),
        90 | 270 => (height, width),
        _ => anyhow::bail!(tr("Неизвестная ориентация шаблона")),
    };
    // Cable templates describe only the flag; the separate wrap is part of the stock length.
    let cable = data["isCable"].as_bool() == Some(true);
    if cable {
        let tail = data["cableLength"]
            .as_f64()
            .context(tr("Нет длины хвостика"))?;
        ensure!(
            tail > 0.0 && feed + tail <= 200.0,
            tr("Некорректная длина кабельной этикетки")
        );
        feed += tail;
    }
    let tenths = |v: f64| -> Result<u16> {
        ensure!(
            (v * 10.0 - (v * 10.0).round()).abs() < 0.001,
            tr("Неточный размер каталога")
        );
        Ok((v * 10.0).round() as u16)
    };
    let size = D101LabelSize::from_tenths(tenths(across)?, tenths(feed)?);
    let name = data["name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| {
            data["names"]
                .as_array()?
                .iter()
                .filter_map(|n| n["name"].as_str())
                .find(|n| !n.trim().is_empty())
        })
        .context(tr("Нет названия расходника"))?;
    // The endpoint supplies nominal capacity in its product name, e.g. T12*40-155白色.
    // Accept that convention only when the dimensions before '-' agree with the template.
    let capacity = capacity_from_name(name, across, feed);
    Ok(RollMetadata {
        barcode: code.into(),
        name: name.chars().take(160).collect(),
        size,
        capacity,
        outline: (cable && size == Some(D101LabelSize::W12_5L109))
            .then_some(LabelOutline::CableFlag),
    })
}
fn capacity_from_name(name: &str, across: f64, feed: f64) -> Option<u16> {
    let (geometry, suffix) = name.split_once('-')?;
    let geometry = geometry.strip_prefix('T')?;
    let (w, l) = geometry.split_once('*')?;
    let w: f64 = w.parse().ok()?;
    let l: f64 = l
        .split('+')
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?
        .iter()
        .sum();
    if !w.is_finite() || !l.is_finite() || (w - across).abs() > 0.01 || (l - feed).abs() > 0.01 {
        return None;
    }
    let digits: String = suffix.chars().take_while(char::is_ascii_digit).collect();
    if suffix[digits.len()..].starts_with(['.', ',']) {
        return None;
    }
    let capacity: u16 = digits.parse().ok()?;
    (capacity > 0).then_some(capacity)
}

pub struct Catalog {
    path: PathBuf,
    cached: BTreeMap<String, RollMetadata>,
    failed: BTreeMap<String, Instant>,
}
impl Catalog {
    pub fn new(path: PathBuf) -> Self {
        let mut cached: BTreeMap<String, RollMetadata> = std::fs::metadata(&path)
            .ok()
            .filter(|m| m.len() <= 16_777_216)
            .and_then(|_| std::fs::read(&path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        cached.retain(|code, m| valid_code(code) && m.barcode == *code && m.name.len() <= 640);
        if cached.len() > 1000 {
            cached.clear();
        }
        // Public product metadata for offline lookup.
        cached
            .entry("6978061950197".into())
            .or_insert(RollMetadata {
                barcode: "6978061950197".into(),
                name: "T12*40-155White".into(),
                size: Some(D101LabelSize::W12L40),
                capacity: Some(155),
                outline: LabelOutline::from_template(
                    crate::label_outline::STOCK_12X40,
                    D101LabelSize::W12L40,
                )
                .ok(),
            });
        // Upgrade metadata cached by versions that did not store the die-cut shape.
        if let Some(known) = cached.get_mut("6978061950197") {
            known.outline = LabelOutline::from_template(
                crate::label_outline::STOCK_12X40,
                D101LabelSize::W12L40,
            )
            .ok();
        }
        Self {
            path,
            cached,
            failed: BTreeMap::new(),
        }
    }
    pub async fn lookup(&mut self, code: &str) -> Result<RollMetadata> {
        ensure!(valid_code(code), tr("Неизвестный формат кода рулона"));
        if let Some(found) = self.cached.get(code) {
            return Ok(found.clone());
        }
        ensure!(
            self.failed
                .get(code)
                .is_none_or(|t| t.elapsed().as_secs() >= 60),
            tr("Каталог пока недоступен; повтор через минуту")
        );
        let requested = code.to_owned();
        let result = tokio::task::spawn_blocking(move || fetch_metadata(&requested)).await?;
        match result {
            Ok(metadata) => {
                self.failed.remove(code);
                if self.cached.len() >= 1000 {
                    self.cached.pop_first();
                }
                self.cached.insert(code.into(), metadata.clone());
                let temporary = self.path.with_extension("json.tmp");
                if let Ok(bytes) = serde_json::to_vec(&self.cached)
                    && std::fs::write(&temporary, bytes).is_ok()
                {
                    let _ = std::fs::rename(temporary, &self.path);
                }
                Ok(metadata)
            }
            Err(e) => {
                if self.failed.len() >= 1000 {
                    self.failed.clear();
                }
                self.failed.insert(code.into(), Instant::now());
                Err(e)
            }
        }
    }
}

pub fn fetch_metadata(code: &str) -> Result<RollMetadata> {
    ensure!(valid_code(code), tr("Некорректный код рулона"));
    let body = serde_json::to_vec(&serde_json::json!({"oneCode":code}))?;
    let bytes = https_request(
        "print.niimbot.com",
        "/api/template/getCloudTemplateByOneCode",
        Some(&body),
        131_072,
    )?;
    let mut metadata = parse_metadata(&bytes, code)?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(size) = metadata.size
        && let Some(url) = json["data"]["backgroundImage"].as_str()
        && let Some(path) = url.strip_prefix("https://oss-print.niimbot.com/")
        && path.starts_with("public_resources/")
        && let Ok(image) = https_request(
            "oss-print.niimbot.com",
            &format!("/{path}"),
            None,
            2_097_152,
        )
    {
        // A cable background contains only its 74-mm flag. Never stretch it over the wrap.
        if let Ok(outline) = LabelOutline::from_template(&image, size) {
            metadata.outline = Some(outline);
        }
    }
    Ok(metadata)
}
fn https_request(server: &str, path: &str, body: Option<&[u8]>, limit: usize) -> Result<Vec<u8>> {
    use std::{
        ffi::c_void,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::Networking::WinHttp::*;
    struct Handle(*mut c_void);
    impl Handle {
        fn new(value: *mut c_void) -> Result<Self> {
            if value.is_null() {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(Self(value))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
    fn check(value: i32) -> Result<()> {
        if value == 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<_>>();
    let method = if body.is_some() { "POST" } else { "GET" };
    let body = body.unwrap_or_default();
    // All handles are owned on this blocking thread; HTTPS certificate checks stay enabled.
    let bytes = unsafe {
        let session = Handle::new(WinHttpOpen(
            wide("D101-Label-Studio/0.1").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        check(WinHttpSetTimeouts(session.0, 3000, 3000, 3000, 3000))?;
        let host = Handle::new(WinHttpConnect(session.0, wide(server).as_ptr(), 443, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            host.0,
            wide(method).as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        check(WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&redirect as *const u32).cast(),
            4,
        ))?;
        let headers = wide(
            "Content-Type: application/json\r\nniimbot-user-agent: AppVersionName/999.0.0\r\n",
        );
        check(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            body.as_ptr().cast(),
            body.len() as u32,
            body.len() as u32,
            0,
        ))?;
        check(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status: u32 = 0;
        let mut length = 4;
        check(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut length,
            null_mut(),
        ))?;
        ensure!(
            status == 200,
            local_format!(
                "Каталог NIIMBOT: HTTP {status}",
                "NIIMBOT catalog: HTTP {status}"
            )
        );
        let started = Instant::now();
        let mut response = Vec::new();
        loop {
            ensure!(
                started.elapsed().as_secs() < 10,
                tr("Тайм-аут каталога NIIMBOT")
            );
            let mut buffer = [0u8; 8192];
            let mut read = 0;
            check(WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            ensure!(
                response.len() + read as usize <= limit,
                tr("Слишком большой ответ каталога")
            );
            response.extend_from_slice(&buffer[..read as usize]);
        }
        response
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    const RESPONSE: &[u8] = include_bytes!("../tests/fixtures/roll-6978061950197.json");
    #[test]
    fn cable_catalog_adds_wrap_and_accepts_verified_alias_and_localized_name() {
        for (bytes, code) in [
            (
                include_bytes!("../tests/fixtures/roll-cable-localized.json").as_slice(),
                "6977031210668",
            ),
            (
                include_bytes!("../tests/fixtures/roll-cable-alias.json").as_slice(),
                "6972842743787",
            ),
        ] {
            let metadata = parse_metadata(bytes, code).unwrap();
            assert_eq!(metadata.barcode, code);
            assert_eq!(metadata.size, Some(D101LabelSize::W12_5L109));
            assert_eq!(metadata.capacity, Some(60));
            assert_eq!(metadata.outline, Some(LabelOutline::CableFlag));
            assert!(parse_metadata(bytes, "unrelated").is_err());
            let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            value["data"]["cableLength"] = 0.into();
            assert!(parse_metadata(&serde_json::to_vec(&value).unwrap(), code).is_err());
            value["data"]["isCable"] = false.into();
            let ordinary = parse_metadata(&serde_json::to_vec(&value).unwrap(), code).unwrap();
            assert_eq!(ordinary.capacity, None);
            assert_eq!(ordinary.size, None);
        }
    }

    #[test]
    fn actual_catalog_reply_gives_dimensions_and_nominal_not_print_limit() {
        let m = parse_metadata(RESPONSE, "6978061950197").unwrap();
        assert_eq!(m.size, Some(D101LabelSize::W12L40));
        assert_eq!(m.capacity, Some(155));
        assert!(parse_metadata(RESPONSE, "wrong-code").is_err());
    }
    #[test]
    fn incorrect_model_orientation_or_name_cannot_invent_size_or_capacity() {
        let mut v: serde_json::Value = serde_json::from_slice(RESPONSE).unwrap();
        v["data"]["name"] = "A random label 155".into();
        assert_eq!(
            parse_metadata(&serde_json::to_vec(&v).unwrap(), "6978061950197")
                .unwrap()
                .capacity,
            None
        );
        v["data"]["name"] = "T14*40-155White".into();
        assert_eq!(
            parse_metadata(&serde_json::to_vec(&v).unwrap(), "6978061950197")
                .unwrap()
                .capacity,
            None
        );
        v["data"]["rotate"] = 45.into();
        assert!(parse_metadata(&serde_json::to_vec(&v).unwrap(), "6978061950197").is_err());
        v["data"]["rotate"] = 90.into();
        v["data"]["profile"]["machineName"] = "B1,D101_PRO".into();
        assert!(parse_metadata(&serde_json::to_vec(&v).unwrap(), "6978061950197").is_err());
    }
    #[tokio::test]
    async fn installed_roll_metadata_is_available_offline_without_consumption_cache() {
        let mut catalog = Catalog::new(PathBuf::from(".test-data/nonexistent-catalog.json"));
        let metadata = catalog.lookup("6978061950197").await.unwrap();
        assert_eq!(metadata.capacity, Some(155));
        assert_eq!(metadata.size, Some(D101LabelSize::W12L40));
        let json = serde_json::to_value(metadata).unwrap();
        assert!(json.get("used").is_none());
        assert!(json.get("remaining").is_none());
    }
}
