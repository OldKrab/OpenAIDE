use openaide_app_server_protocol::server_requests::QuestionStringFormat;
use regex::Regex;

pub(super) fn valid_format(format: QuestionStringFormat, value: &str) -> bool {
    match format {
        QuestionStringFormat::Email => value
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.')),
        QuestionStringFormat::Uri => value
            .split_once(':')
            .is_some_and(|(scheme, rest)| !scheme.is_empty() && !rest.is_empty()),
        QuestionStringFormat::Date => Regex::new(r"^\d{4}-\d{2}-\d{2}$")
            .expect("static date expression is valid")
            .is_match(value),
        QuestionStringFormat::DateTime => {
            value.contains('T') && (value.ends_with('Z') || value.contains('+'))
        }
    }
}
