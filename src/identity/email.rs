use std::collections::HashSet;
use std::sync::LazyLock;

static DISPOSABLE: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("../../resources/disposable-domains.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

pub fn validate(address: &str) -> Result<(), &'static str> {
    if address.len() > 254 || !address.is_ascii() {
        return Err("Enter a valid email address");
    }
    let (local, domain) = address
        .split_once('@')
        .ok_or("Enter a valid email address")?;
    if local.is_empty()
        || local.len() > 64
        || !local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b))
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || domain.len() > 253
        || domain.contains('@')
        || !domain.contains('.')
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err("Enter a valid email address");
    }
    if DISPOSABLE.contains(domain)
        || domain
            .match_indices('.')
            .any(|(index, _)| DISPOSABLE.contains(&domain[index + 1..]))
    {
        return Err("Please use a lasting email address to create an account");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_known_disposable_domains_and_subdomains() {
        assert!(validate("rider@mailinator.com").is_err());
        assert!(validate("rider@box.mailinator.com").is_err());
        assert!(validate("rider@gmail.com").is_ok());
    }

    #[test]
    fn rejects_invalid_addresses() {
        for address in [
            "rider@@example.com",
            "rider@example",
            "rider@-bad.com",
            "rider..a@example.com",
        ] {
            assert!(validate(address).is_err(), "{address}");
        }
    }
}
