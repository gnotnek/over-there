"""Refresh the local, compiled-in disposable-mail domain list (no paid API)."""

from pathlib import Path
from urllib.request import urlopen

SOURCE = "https://raw.githubusercontent.com/groundcat/disposable-email-domain-list/main/domains.txt"
DESTINATION = Path(__file__).resolve().parents[1] / "resources/disposable-domains.txt"


def main() -> None:
    with urlopen(SOURCE, timeout=20) as response:
        raw = response.read().decode("utf-8")
    domains = sorted(set(raw.splitlines()))
    if len(domains) < 1_000 or any(
        not domain or domain != domain.strip().lower() or "." not in domain
        for domain in domains
    ):
        raise SystemExit("Upstream list was unexpectedly small or malformed; keeping the old list")
    header = (
        "# Source: groundcat/disposable-email-domain-list (MIT). "
        "Refresh with scripts/update_disposable_domains.py.\n"
    )
    DESTINATION.write_text(header + "\n".join(domains) + "\n")
    print(f"Updated {len(domains)} domains. Rebuild the Rust application to use this list.")


if __name__ == "__main__":
    main()
