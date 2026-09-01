const REDACTED: &str = "[REDACTED]";

pub fn redact_sensitive(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied = 0;
    let mut index = 0;

    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic()
            || (index > 0 && is_name_byte(bytes[index.saturating_sub(1)]))
        {
            index += 1;
            continue;
        }

        let name_start = index;
        while index < bytes.len() && is_name_byte(bytes[index]) {
            index += 1;
        }
        let name = &input[name_start..index];
        if !is_sensitive_name(name) {
            continue;
        }

        let mut delimiter = index;
        if matches!(bytes.get(delimiter).copied(), Some(b'\"' | b'\'')) {
            delimiter += 1;
        }
        while bytes.get(delimiter).is_some_and(u8::is_ascii_whitespace) {
            delimiter += 1;
        }
        if !matches!(bytes.get(delimiter).copied(), Some(b'=' | b':')) {
            continue;
        }

        let mut value_start = delimiter + 1;
        while bytes.get(value_start).is_some_and(u8::is_ascii_whitespace) {
            value_start += 1;
        }
        let quote = bytes.get(value_start).copied().filter(|byte| matches!(byte, b'\"' | b'\''));
        if quote.is_some() {
            value_start += 1;
        }
        if value_start >= bytes.len() {
            continue;
        }

        if quote.is_none() && name.eq_ignore_ascii_case("authorization") {
            for scheme in ["Bearer ", "Basic ", "Client-ID "] {
                if input[value_start..]
                    .get(..scheme.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
                {
                    value_start += scheme.len();
                    break;
                }
            }
        }

        let value_end = match quote {
            Some(quote) => quoted_value_end(bytes, value_start, quote),
            None => unquoted_value_end(bytes, value_start),
        };
        if value_end == value_start {
            continue;
        }

        output.push_str(&input[copied..value_start]);
        output.push_str(REDACTED);
        copied = value_end;
        index = value_end;
    }

    if copied == 0 {
        return input.to_string();
    }
    output.push_str(&input[copied..]);
    output
}

pub fn redact_known_secrets(input: &str, secrets: &[&str]) -> String {
    let mut output = redact_sensitive(input);
    for secret in secrets.iter().copied().filter(|secret| secret.len() >= 4) {
        output = output.replace(secret, REDACTED);
    }
    output
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_sensitive_name(name: &str) -> bool {
    let normalized: String = name
        .bytes()
        .filter(|byte| !matches!(byte, b'_' | b'-'))
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect();
    matches!(
        normalized.as_str(),
        "apikey"
            | "xapikey"
            | "key"
            | "token"
            | "accesstoken"
            | "clientsecret"
            | "secret"
            | "password"
            | "passwd"
            | "authorization"
    )
}

fn quoted_value_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == quote && !escaped {
            break;
        }
        escaped = byte == b'\\' && !escaped;
        if byte != b'\\' {
            escaped = false;
        }
        index += 1;
    }
    index
}

fn unquoted_value_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len()
        && !bytes[index].is_ascii_whitespace()
        && !matches!(bytes[index], b'&' | b'#' | b',' | b';' | b'}' | b']')
    {
        index += 1;
    }
    index
}

#[cfg(test)]
#[path = "redaction_tests.rs"]
mod tests;
