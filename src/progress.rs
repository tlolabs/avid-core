use std::collections::HashMap;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Validating,
    Probing,
    Compositing,
    Encoding,
    Publishing,
    Complete,
}
impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validating => "validating",
            Self::Probing => "probing",
            Self::Compositing => "compositing",
            Self::Encoding => "encoding",
            Self::Publishing => "publishing",
            Self::Complete => "complete",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    pub elapsed_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub fraction: Option<f64>,
    pub eta_seconds: Option<f64>,
}
/// Called on the operation's worker thread. Callbacks must return promptly and must not panic.
/// Progress can be coalesced; automatic fallback restarts progress for the software attempt.
pub trait EventSink: Send + Sync {
    fn stage(&self, _stage: Stage) {}
    fn progress(&self, _progress: Progress) {}
    fn diagnostic(&self, _line: &str) {}
}
impl EventSink for () {}

pub(crate) fn parse(values: &HashMap<String, String>, duration: Option<f64>) -> Progress {
    let duration = duration.filter(|v| v.is_finite() && *v > 0.0);
    let elapsed = values
        .get("out_time_us")
        .or_else(|| values.get("out_time_ms"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / 1_000_000.0)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .or_else(|| values.get("out_time").and_then(|v| timestamp(v)));
    let speed = values
        .get("speed")
        .and_then(|v| v.trim_end_matches('x').parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0);
    Progress {
        elapsed_seconds: elapsed,
        duration_seconds: duration,
        fraction: elapsed.zip(duration).map(|(e, d)| (e / d).clamp(0.0, 1.0)),
        eta_seconds: elapsed
            .zip(duration)
            .zip(speed)
            .map(|((e, d), s)| (d - e).max(0.0) / s),
    }
}
fn timestamp(value: &str) -> Option<f64> {
    let parts: Vec<_> = value.trim().split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut total = 0.0;
    for part in parts {
        let n = part.parse::<f64>().ok()?;
        if !n.is_finite() || n < 0.0 {
            return None;
        }
        total = total * 60.0 + n;
    }
    total.is_finite().then_some(total)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_units_eta_and_invalid_values() {
        let mut v = HashMap::from([
            ("out_time_ms".into(), "2000000".into()),
            ("speed".into(), "2x".into()),
        ]);
        let p = parse(&v, Some(10.0));
        assert_eq!(p.fraction, Some(0.2));
        assert_eq!(p.eta_seconds, Some(4.0));
        v.insert("out_time_us".into(), "NaN".into());
        v.insert("out_time".into(), "00:01:02.5".into());
        assert_eq!(parse(&v, Some(0.0)).elapsed_seconds, Some(62.5));
        assert_eq!(parse(&v, Some(0.0)).fraction, None);
        assert_eq!(timestamp("00:00:inf"), None);
        assert_eq!(timestamp("00:00:-1"), None);
        assert_eq!(timestamp("01:02"), Some(62.0));
    }
}
