use crate::{
    bitmap::{Bitmap, LabelSpec},
    catalog::{Catalog, RollMetadata},
    connection::{BluetoothConnector, Connection},
    driver::PrintOptions,
    printer_status::{PrinterReading, PrinterStatus, StatusPhase},
};
use crate::{i18n::tr, local_format};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::Notify;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: crate::i18n::Language,
    pub address: String,
    pub device_name: String,
    pub label: LabelSpec,
    pub print: PrintOptions,
    /// Registration in physical head dots by Bluetooth address. Missing means zero.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub vertical_offsets: BTreeMap<String, i8>,
}

pub const MAX_OFFSET_DOTS: i8 = 16;

impl Config {
    pub fn vertical_offset(&self) -> i8 {
        self.vertical_offsets
            .get(&self.address.to_ascii_uppercase())
            .copied()
            .unwrap_or(0)
    }

    pub fn set_vertical_offset(&mut self, dots: i8) {
        let key = self.address.to_ascii_uppercase();
        if dots == 0 {
            self.vertical_offsets.remove(&key);
        } else {
            self.vertical_offsets
                .insert(key, dots.clamp(-MAX_OFFSET_DOTS, MAX_OFFSET_DOTS));
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: i32,
    pub name: String,
    /// IPP job-state: pending=3, held=4, processing=5, canceled=7, aborted=8, completed=9.
    pub state: i32,
    pub message: String,
    pub completed: usize,
    pub total: usize,
    pub copies: usize,
    pub pages: Vec<Bitmap>,
    pub config: Config,
}
impl Job {
    pub fn summary(&self) -> Self {
        Self {
            pages: Vec::new(),
            ..self.clone_without_pages()
        }
    }
    fn clone_without_pages(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            state: self.state,
            message: self.message.clone(),
            completed: self.completed,
            total: self.total,
            copies: self.copies,
            pages: Vec::new(),
            config: self.config.clone(),
        }
    }
    pub fn state_text(&self) -> &'static str {
        match self.state {
            3 => tr("В очереди"),
            4 => tr("Приём документа"),
            5 => tr("Печать"),
            7 => tr("Отменено"),
            8 => tr("Ошибка"),
            9 => tr("Готово"),
            _ => tr("Неизвестно"),
        }
    }
}
struct Inner {
    config: Config,
    jobs: BTreeMap<i32, Job>,
    next_id: i32,
    active: Option<(i32, Arc<AtomicBool>)>,
    server_status: String,
    printer_status: PrinterStatus,
    printer_revision: u64,
}

pub struct Engine {
    _directory_lock: std::fs::File,
    inner: Mutex<Inner>,
    wake: Notify,
    status_wake: Notify,
    // One BLE exchange at a time: background telemetry must not consume print replies.
    printer_access: tokio::sync::Mutex<Connection<BluetoothConnector>>,
    catalog: tokio::sync::Mutex<Catalog>,
    status_refresh_requested: AtomicBool,
    stopping: AtomicBool,
    pub directory: PathBuf,
    pub simulated: bool,
    pub port: u16,
    pub started: Instant,
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&temporary)?;
    serde_json::to_writer(&mut file, value)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, path)?;
    Ok(())
}

impl Engine {
    pub fn open(directory: PathBuf, simulated: bool, port: u16) -> Result<Arc<Self>> {
        std::fs::create_dir_all(directory.join("jobs"))?;
        let directory_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("bridge.lock"))?;
        directory_lock.try_lock().context(tr(
            "Этот каталог данных уже используется другим экземпляром приложения",
        ))?;
        let config_path = directory.join("config.json");
        let config: Config = if config_path.exists() {
            serde_json::from_slice(&std::fs::read(config_path)?)?
        } else {
            Config::default()
        };
        config.language.activate();
        let mut jobs = BTreeMap::new();
        for entry in std::fs::read_dir(directory.join("jobs"))? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let mut job: Job =
                serde_json::from_slice(&std::fs::read(&path)?).with_context(|| {
                    local_format!(
                        "Повреждён файл задания {}",
                        "Damaged job file {}",
                        path.display()
                    )
                })?;
            // A crash can occur after the physical page prints and before it is recorded.
            // Never replay uncertain jobs, including a PrintStart that was not acknowledged.
            if job.state < 7 {
                job.state = 8;
                job.message=tr("Приложение было закрыто. Проверьте уже напечатанные этикетки перед повторной отправкой").into();
                job.pages.clear();
                write_json(&path, &job)?;
            }
            jobs.insert(job.id, job);
        }
        let next_id = jobs.keys().next_back().copied().unwrap_or(0) + 1;
        while jobs.len() > 100 {
            jobs.pop_first();
        }
        Ok(Arc::new(Self {
            _directory_lock: directory_lock,
            inner: Mutex::new(Inner {
                printer_status: PrinterStatus::new(simulated, !config.address.is_empty()),
                printer_revision: 0,
                config,
                jobs,
                next_id,
                active: None,
                server_status: tr("Запуск IPP…").into(),
            }),
            wake: Notify::new(),
            status_wake: Notify::new(),
            printer_access: tokio::sync::Mutex::new(Connection::new(BluetoothConnector)),
            catalog: tokio::sync::Mutex::new(Catalog::new(directory.join("roll-catalog.json"))),
            status_refresh_requested: AtomicBool::new(true),
            stopping: AtomicBool::new(false),
            directory,
            simulated,
            port,
            started: Instant::now(),
        }))
    }
    fn save_job(&self, job: &Job) -> Result<()> {
        write_json(
            &self.directory.join("jobs").join(format!("{}.json", job.id)),
            job,
        )
    }
    pub fn config(&self) -> Config {
        self.inner.lock().unwrap().config.clone()
    }
    pub fn set_language(&self, language: crate::i18n::Language) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let mut config = inner.config.clone();
        config.language = language;
        write_json(&self.directory.join("config.json"), &config)?;
        inner.config = config;
        language.activate();
        self.request_status_refresh();
        Ok(())
    }
    pub fn set_config(&self, config: Config) -> Result<()> {
        config.label.dimensions()?;
        config.label.supported_size()?;
        config.label.mask()?;
        ensure!(
            (1..=3).contains(&config.print.density) && matches!(config.print.label_type, 1 | 5),
            tr("Неверные параметры D101")
        );
        let mut inner = self.inner.lock().unwrap();
        write_json(&self.directory.join("config.json"), &config)?;
        if !inner.config.address.eq_ignore_ascii_case(&config.address) {
            inner.printer_revision = inner.printer_revision.wrapping_add(1);
            inner.printer_status = PrinterStatus::new(self.simulated, !config.address.is_empty());
        }
        inner.config = config;
        self.request_status_refresh();
        Ok(())
    }
    pub fn printer_status(&self) -> PrinterStatus {
        let inner = self.inner.lock().unwrap();
        let mut status = inner.printer_status.clone();
        status.printing = inner.active.as_ref().is_some_and(|(id, _)| {
            inner
                .jobs
                .get(id)
                .is_some_and(|j| j.config.address.eq_ignore_ascii_case(&inner.config.address))
        });
        status
    }
    pub fn request_status_refresh(&self) {
        self.status_refresh_requested.store(true, Ordering::Relaxed);
        self.status_wake.notify_one();
    }
    pub fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some((_, cancel)) = &self.inner.lock().unwrap().active {
            cancel.store(true, Ordering::Relaxed);
        }
        self.wake.notify_one();
        self.status_wake.notify_one();
    }
    pub async fn disconnect_printer(&self) {
        self.printer_access.lock().await.disconnect().await;
    }
    pub async fn monitor_printer(self: Arc<Self>) {
        while !self.stopping.load(Ordering::Relaxed) {
            self.refresh_printer_status().await;
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {},
                _ = self.status_wake.notified() => {},
            }
        }
    }
    async fn refresh_printer_status(self: &Arc<Self>) {
        if self.simulated || self.stopping.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut connection) = self.printer_access.try_lock() else {
            return;
        };
        let (address, revision, full_status) = {
            let mut inner = self.inner.lock().unwrap();
            // Pending jobs take precedence, including a worker waiting on this lock.
            if inner.active.is_some() || inner.jobs.values().any(|j| j.state == 3) {
                return;
            }
            let forced = self.status_refresh_requested.swap(false, Ordering::Relaxed);
            let connected = connection.connected_to(&inner.config.address);
            let full_status = forced
                || !connected
                || inner
                    .printer_status
                    .updated
                    .is_none_or(|t| t.elapsed().as_secs() >= 30);
            if full_status && !inner.config.address.is_empty() {
                inner.printer_status.phase = if connected {
                    StatusPhase::Updating
                } else if matches!(
                    inner.printer_status.phase,
                    StatusPhase::Unavailable | StatusPhase::Reconnecting
                ) {
                    StatusPhase::Reconnecting
                } else {
                    StatusPhase::Connecting
                };
            }
            (
                inner.config.address.clone(),
                inner.printer_revision,
                full_status,
            )
        };
        if address.is_empty() {
            connection.disconnect().await;
            return;
        }
        if full_status {
            let result = connection.read_status(&address).await;
            let for_catalog = result.as_ref().ok().cloned();
            self.publish_printer_status(revision, result);
            if let Some(reading) = for_catalog
                && let Some(roll) = &reading.roll
            {
                let code = roll.barcode.clone();
                let engine = self.clone();
                // HTTP must not hold the Bluetooth lock or delay the keepalive / print worker.
                tokio::spawn(async move {
                    let result = engine.catalog.lock().await.lookup(&code).await;
                    engine.publish_catalog(revision, &reading, result);
                });
            }
        } else if let Err(e) = connection.ping(&address).await {
            self.publish_printer_status(revision, Err(e));
        }
        // A selection change can arrive during connection establishment or a query.
        let obsolete = self.inner.lock().unwrap().printer_revision != revision;
        if obsolete {
            connection.disconnect().await;
        }
    }
    fn publish_printer_status(&self, revision: u64, result: Result<PrinterReading>) {
        let mut inner = self.inner.lock().unwrap();
        // A slow reply from a previously selected printer must never replace the new one.
        if inner.printer_revision != revision {
            return;
        }
        inner.printer_status = match result {
            Ok(reading) => PrinterStatus {
                phase: StatusPhase::Available,
                reading: Some(reading),
                updated: Some(Instant::now()),
                error: None,
                printing: false,
            },
            Err(e) => PrinterStatus {
                phase: StatusPhase::Unavailable,
                error: Some(format!("{e:#}")),
                ..PrinterStatus::new(false, true)
            },
        };
    }
    fn publish_catalog(
        &self,
        revision: u64,
        expected: &PrinterReading,
        result: Result<RollMetadata>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        if self.stopping.load(Ordering::Relaxed)
            || inner.printer_revision != revision
            || inner.active.is_some()
            || inner.printer_status.reading.as_ref() != Some(expected)
        {
            return;
        }
        let reading = inner.printer_status.reading.as_mut().unwrap();
        match result {
            Ok(metadata) => {
                if reading.paper.is_none() && metadata.size.is_some() {
                    reading.notices.retain(|s| {
                        !s.starts_with(tr("Размер рулона недоступен:"))
                            && !s.starts_with(tr("Прошивка не сообщает размер"))
                    });
                }
                reading.catalog = Some(metadata);
            }
            Err(e) => reading.notices.push(local_format!(
                "Каталог рулонов недоступен: {e:#}",
                "Roll catalog unavailable: {e:#}"
            )),
        }
    }
    pub fn ready(&self) -> bool {
        self.simulated || !self.config().address.is_empty()
    }
    pub fn jobs(&self) -> Vec<Job> {
        self.inner
            .lock()
            .unwrap()
            .jobs
            .values()
            .rev()
            .map(Job::summary)
            .collect()
    }
    pub fn job(&self, id: i32) -> Option<Job> {
        self.inner.lock().unwrap().jobs.get(&id).map(Job::summary)
    }
    pub fn server_status(&self) -> String {
        self.inner.lock().unwrap().server_status.clone()
    }
    pub fn set_server_status(&self, status: String) {
        self.inner.lock().unwrap().server_status = status;
    }
    pub fn create_job(
        &self,
        name: String,
        copies: usize,
        pages: Option<Vec<Bitmap>>,
    ) -> Result<i32> {
        ensure!(
            !self.stopping.load(Ordering::Relaxed),
            tr("Приложение закрывается")
        );
        ensure!(self.ready(), tr("Выберите Bluetooth-принтер в приложении"));
        ensure!((1..=1000).contains(&copies), tr("Количество копий: 1–1000"));
        let has_document = pages.is_some();
        let pages = pages.unwrap_or_default();
        if has_document {
            validate_pages(&pages, copies)?;
        }
        let mut inner = self.inner.lock().unwrap();
        ensure!(
            inner.jobs.values().filter(|j| j.state < 7).count() < 32,
            tr("Очередь заполнена (32 задания)")
        );
        let occupied: usize = inner
            .jobs
            .values()
            .flat_map(|j| &j.pages)
            .map(|p| p.bits.len())
            .sum();
        ensure!(
            occupied + pages.iter().map(|p| p.bits.len()).sum::<usize>() <= 64 * 1024 * 1024,
            tr("Очередь превышает 64 МБ")
        );
        let id = inner.next_id;
        let job = Job {
            id,
            name: name.chars().take(200).collect(),
            state: if has_document { 3 } else { 4 },
            message: String::new(),
            completed: 0,
            total: pages.len() * copies,
            copies,
            pages,
            config: inner.config.clone(),
        };
        self.save_job(&job)?;
        inner.next_id += 1;
        inner.jobs.insert(id, job);
        self.wake.notify_one();
        Ok(id)
    }
    pub fn supply_document(&self, id: i32, pages: Vec<Bitmap>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let occupied: usize = inner
            .jobs
            .values()
            .flat_map(|j| &j.pages)
            .map(|p| p.bits.len())
            .sum();
        ensure!(
            occupied + pages.iter().map(|p| p.bits.len()).sum::<usize>() <= 64 * 1024 * 1024,
            tr("Очередь превышает 64 МБ")
        );
        let job = inner.jobs.get(&id).context(tr("Задание не найдено"))?;
        ensure!(
            job.state == 4,
            tr("Документ уже принят или задание завершено")
        );
        validate_pages(&pages, job.copies)?;
        let mut job = job.clone();
        job.total = pages.len() * job.copies;
        job.pages = pages;
        job.state = 3;
        self.save_job(&job)?;
        inner.jobs.insert(id, job);
        self.wake.notify_one();
        Ok(())
    }
    pub fn cancel(&self, id: i32) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.jobs.get(&id).context(tr("Задание не найдено"))?;
        ensure!(job.state < 7, tr("Задание уже завершено"));
        if let Some((active_id, cancel)) = &inner.active
            && *active_id == id
        {
            cancel.store(true, Ordering::Relaxed);
            return Ok(());
        }
        let mut job = job.clone();
        job.state = 7;
        job.message = tr("Отменено пользователем").into();
        job.pages.clear();
        self.save_job(&job)?;
        inner.jobs.insert(id, job);
        Ok(())
    }
    fn update_job(&self, id: i32, f: impl FnOnce(&mut Job)) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let job = inner.jobs.get_mut(&id).context(tr("Задание не найдено"))?;
        f(job);
        self.save_job(job)
    }
    pub async fn worker(self: Arc<Self>) {
        while !self.stopping.load(Ordering::Relaxed) {
            let notified = self.wake.notified();
            let next = {
                let mut inner = self.inner.lock().unwrap();
                let job = inner.jobs.values_mut().find(|j| j.state == 3);
                if let Some(job) = job {
                    job.state = 5;
                    if let Err(e) = self.save_job(job) {
                        job.state = 8;
                        job.message = local_format!(
                            "Не удалось сохранить состояние: {e}",
                            "Could not save state: {e}"
                        );
                        None
                    } else {
                        let job = job.clone();
                        let cancel = Arc::new(AtomicBool::new(false));
                        inner.active = Some((job.id, cancel.clone()));
                        Some((job, cancel))
                    }
                } else {
                    None
                }
            };
            let Some((mut job, cancel)) = next else {
                notified.await;
                continue;
            };
            let result = self.run_job(&mut job, cancel.clone()).await;
            let connection_error = result.as_ref().err().map(|e| format!("{e:#}"));
            let cancelled = cancel.load(Ordering::Relaxed);
            let save = self.update_job(job.id, |j| {
                j.state = if result.is_ok() {
                    9
                } else if cancelled {
                    7
                } else {
                    8
                };
                j.message = match result {
                    Ok(()) => {
                        if self.simulated {
                            tr("Симуляция: PNG сохранены, принтер не использовался").into()
                        } else {
                            tr("D101 подтвердил завершение").into()
                        }
                    }
                    Err(e) => format!("{e:#}"),
                };
                j.pages.clear();
            });
            if let Err(e) = save {
                self.set_server_status(local_format!(
                    "Ошибка сохранения очереди: {e}",
                    "Could not save the queue: {e}"
                ));
            }
            {
                let mut inner = self.inner.lock().unwrap();
                inner.active = None;
                if job
                    .config
                    .address
                    .eq_ignore_ascii_case(&inner.config.address)
                {
                    // Discard pre-print counters instead of guessing how much paper was used.
                    inner.printer_status =
                        PrinterStatus::new(self.simulated, !inner.config.address.is_empty());
                    if !self.simulated
                        && let Some(error) = connection_error
                    {
                        inner.printer_status.phase = StatusPhase::Unavailable;
                        inner.printer_status.error = Some(error);
                    }
                }
            }
            self.request_status_refresh();
        }
    }
    async fn run_job(&self, job: &mut Job, cancel: Arc<AtomicBool>) -> Result<()> {
        // Work on the worker's private copy. Both IPP and Studio arrive in head
        // coordinates here; apply the job's registration once before copying.
        let offset = job.config.vertical_offset();
        ensure!(
            (-MAX_OFFSET_DOTS..=MAX_OFFSET_DOTS).contains(&offset),
            tr("Недопустимая поправка печати")
        );
        for page in &mut job.pages {
            page.shift_head_columns(offset)?;
        }
        // Expand copies in collated order; the driver combines only adjacent equal
        // rasters into a bounded firmware copy count (at most 1000 labels per job).
        let pages: Vec<_> = (0..job.copies)
            .flat_map(|_| job.pages.iter().cloned())
            .collect();
        if self.simulated {
            let folder = self.directory.join("simulated").join(job.id.to_string());
            std::fs::create_dir_all(&folder)?;
            for (n, page) in pages.iter().enumerate() {
                ensure!(!cancel.load(Ordering::Relaxed), tr("Отменено"));
                page.to_gray()
                    .save(folder.join(format!("{:04}.png", n + 1)))?;
                self.update_job(job.id, |j| j.completed = n + 1)?;
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
            return Ok(());
        }
        let mut connection = self.printer_access.lock().await;
        ensure!(!cancel.load(Ordering::Relaxed), tr("Печать отменена"));
        connection
            .print(
                &job.config.address,
                &pages,
                &job.config.print,
                cancel,
                |n| {
                    if let Err(e) = self.update_job(job.id, |j| j.completed = n) {
                        self.set_server_status(local_format!(
                            "Ошибка записи прогресса: {e}",
                            "Could not save progress: {e}"
                        ));
                    }
                },
            )
            .await
    }
}

fn validate_pages(pages: &[Bitmap], copies: usize) -> Result<()> {
    ensure!(
        !pages.is_empty() && pages.len().checked_mul(copies).is_some_and(|n| n <= 1000),
        tr("Максимум 1000 этикеток в задании")
    );
    for page in pages {
        page.validate()?;
    }
    ensure!(
        pages.iter().map(|p| p.bits.len()).sum::<usize>() * copies <= 32 * 1024 * 1024,
        tr("Задание превышает 32 МБ растра")
    );
    Ok(())
}

#[cfg(test)]
mod status_tests {
    use super::*;
    fn engine(simulated: bool) -> Arc<Engine> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Engine::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".test-data")
                .join(format!("status-{stamp}")),
            simulated,
            18631,
        )
        .unwrap()
    }
    fn reading() -> PrinterReading {
        PrinterReading {
            connection_id: 1,
            model: "NIIMBOT D101".into(),
            battery_percent: Some(75),
            roll: None,
            paper: None,
            catalog: None,
            notices: vec![],
        }
    }
    #[tokio::test]
    async fn simulation_and_unconfigured_devices_do_not_attempt_bluetooth() {
        let simulation = engine(true);
        simulation
            .set_config(Config {
                address: "simulated".into(),
                ..Default::default()
            })
            .unwrap();
        simulation.refresh_printer_status().await;
        assert_eq!(simulation.printer_status().phase, StatusPhase::Simulation);
        let unconfigured = engine(false);
        unconfigured.refresh_printer_status().await;
        assert_eq!(
            unconfigured.printer_status().phase,
            StatusPhase::NotConfigured
        );
    }
    #[tokio::test]
    async fn busy_or_queued_printing_prevents_a_status_connection() {
        let engine = engine(false);
        engine
            .set_config(Config {
                address: "not-a-real-device".into(),
                ..Default::default()
            })
            .unwrap();
        let access = engine.printer_access.lock().await;
        engine.refresh_printer_status().await;
        assert_eq!(engine.printer_status().phase, StatusPhase::Waiting);
        drop(access);
        engine
            .create_job(
                "queued".into(),
                1,
                Some(vec![Bitmap {
                    width: 8,
                    height: 1,
                    bits: vec![0],
                }]),
            )
            .unwrap();
        engine.refresh_printer_status().await;
        assert_eq!(engine.printer_status().phase, StatusPhase::Waiting);
    }
    #[test]
    fn late_catalog_reply_cannot_restore_disconnected_or_replaced_printer_status() {
        let engine = engine(false);
        let mut expected = reading();
        expected.roll = Some(crate::printer_status::RollInfo {
            uid: [1; 8],
            barcode: "6978061950197".into(),
            print_limit: 186,
            used: 95,
            capacity: None,
            label_type: 1,
        });
        let metadata = crate::catalog::parse_metadata(
            include_bytes!("../tests/fixtures/roll-6978061950197.json"),
            "6978061950197",
        )
        .unwrap();
        engine.publish_printer_status(0, Ok(expected.clone()));
        engine.publish_catalog(0, &expected, Ok(metadata.clone()));
        assert_eq!(engine.printer_status().remaining_text(), "60 из 155 шт.");
        engine.publish_printer_status(0, Err(anyhow::anyhow!("lost")));
        engine.publish_catalog(0, &expected, Ok(metadata.clone()));
        assert_eq!(engine.printer_status().remaining_text(), "Нет данных");
        let mut newer = expected.clone();
        newer.roll.as_mut().unwrap().used = 96;
        engine.publish_printer_status(0, Ok(newer));
        engine.publish_catalog(0, &expected, Ok(metadata));
        assert!(engine.printer_status().reading.unwrap().catalog.is_none());
    }
    #[test]
    fn changing_printer_rejects_old_replies_and_failures_clear_stale_values() {
        let engine = engine(false);
        engine
            .set_config(Config {
                address: "A".into(),
                ..Default::default()
            })
            .unwrap();
        let first = engine.inner.lock().unwrap().printer_revision;
        engine.publish_printer_status(first, Ok(reading()));
        assert_eq!(engine.printer_status().battery_text(), "75 %");
        engine
            .set_config(Config {
                address: "B".into(),
                ..Default::default()
            })
            .unwrap();
        engine
            .set_config(Config {
                address: "A".into(),
                ..Default::default()
            })
            .unwrap();
        engine.publish_printer_status(first, Ok(reading()));
        assert_eq!(engine.printer_status().battery_text(), "Нет данных");
        assert_eq!(engine.printer_status().phase, StatusPhase::Waiting);
        let current = engine.inner.lock().unwrap().printer_revision;
        engine.publish_printer_status(current, Ok(reading()));
        engine.publish_printer_status(current, Err(anyhow::anyhow!("disconnected")));
        assert_eq!(engine.printer_status().phase, StatusPhase::Unavailable);
        assert_eq!(engine.printer_status().battery_text(), "Нет данных");
        assert!(engine.printer_status().reading.is_none());
    }
}
