use crate::gcp_logging::GcpLoggerClient;
use log::{Level, Metadata, Record};

pub struct SimpleLogger {
    gcp_client: Option<GcpLoggerClient>,
}

impl SimpleLogger {
    pub fn init() {
        let gcp_client = GcpLoggerClient::new();
        if gcp_client.is_none() {
            println!("GCP Logging disabled: credentials not found.");
        }

        let level_filter = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.parse::<log::LevelFilter>().ok())
            .unwrap_or(log::LevelFilter::Info);

        let logger = SimpleLogger { gcp_client };
        log::set_boxed_logger(Box::new(logger))
            .map(|()| log::set_max_level(level_filter))
            .expect("Failed to initialize logger");
    }
}

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level = record.level();
            let message = format!("{}", record.args());

            // Console output
            println!("[{}] {}", level, message);

            // GCP output for INFO and above
            if let (true, Some(client)) = (level <= Level::Info, self.gcp_client.as_ref()) {
                let severity = match level {
                    Level::Error => {
                        if message.contains("[FATAL]") {
                            "CRITICAL"
                        } else {
                            "ERROR"
                        }
                    }
                    Level::Warn => "WARNING",
                    Level::Info => "INFO",
                    _ => "DEBUG",
                };
                client.log(severity, &message);
            }
        }
    }

    fn flush(&self) {}
}
