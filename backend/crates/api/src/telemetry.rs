use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::LogFormat;

const SERVICE_NAME: &str = "teach-me";
const TRACES_PATH: &str = "/v1/traces";

/// Keeps the OTLP exporter alive; `shutdown` flushes the spans still queued.
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl TelemetryGuard {
    /// Whether spans are being exported.
    #[must_use]
    pub const fn exporting(&self) -> bool {
        self.provider.is_some()
    }

    /// Flushes and stops the exporter. A no-op when export is off.
    pub fn shutdown(self) {
        if let Some(provider) = self.provider
            && let Err(e) = provider.shutdown()
        {
            tracing::warn!(error = %e, "could not flush the traces at shutdown");
        }
    }
}

/// Installs the global subscriber. `RUST_LOG` overrides the default filter.
/// With `otlp_endpoint` set, spans are also exported over OTLP/HTTP (protobuf);
/// without it the subscriber is exactly the plain log one.
/// The token is never passed to any log statement.
///
/// # Errors
/// The OTLP exporter cannot be built (invalid endpoint).
pub fn init(format: LogFormat, otlp_endpoint: Option<&str>) -> anyhow::Result<TelemetryGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let provider = build_tracer_provider(otlp_endpoint)?;
    let otel = provider
        .as_ref()
        .map(|p| tracing_opentelemetry::layer().with_tracer(p.tracer(SERVICE_NAME)));
    let registry = tracing_subscriber::registry().with(filter).with(otel);
    match format {
        LogFormat::Text => registry.with(tracing_subscriber::fmt::layer()).init(),
        LogFormat::Json => registry
            .with(tracing_subscriber::fmt::layer().json())
            .init(),
    }
    Ok(TelemetryGuard { provider })
}

/// `None` when no endpoint is set: nothing is built, nothing is exported.
fn build_tracer_provider(endpoint: Option<&str>) -> anyhow::Result<Option<SdkTracerProvider>> {
    let Some(endpoint) = endpoint.map(str::trim).filter(|e| !e.is_empty()) else {
        return Ok(None);
    };
    let url = if endpoint.ends_with(TRACES_PATH) {
        endpoint.to_owned()
    } else {
        format!("{}{TRACES_PATH}", endpoint.trim_end_matches('/'))
    };
    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(url)
        .build()?;
    let resource = Resource::builder()
        .with_service_name(SERVICE_NAME)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();
    Ok(Some(
        SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_endpoint_installs_no_export() {
        assert!(build_tracer_provider(None).unwrap().is_none());
        assert!(build_tracer_provider(Some("")).unwrap().is_none());
        assert!(build_tracer_provider(Some("  ")).unwrap().is_none());
    }
}
