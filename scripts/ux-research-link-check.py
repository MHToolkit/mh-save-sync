#!/usr/bin/env python3
"""Verify that UI/UX research sources are reachable and record lightweight evidence.

The script intentionally stores only source metadata (status, final URL, title)
and never page bodies.  It is a manual evidence gate for docs/research rather
than a mandatory CI step because external design/help sites can rate-limit or
change independently from save-sync code.
"""

from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import re
import socket
import ssl
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DOC = ROOT / "docs/research/UI_UX_PATTERNS.md"
DEFAULT_OUTPUT = ROOT / "artifacts/research/ui_ux_link_check.json"

URL_RE = re.compile(r"https://[^\s|)]+")
TITLE_RE = re.compile(rb"<title[^>]*>(.*?)</title>", re.I | re.S)
MAX_READ_BYTES = 512 * 1024
SOURCES_HEADINGS = {"## Sources reviewed", "## Sources"}


def extract_source_urls(markdown: str) -> list[str]:
    """Extract source URLs from recognized Sources sections only, in order."""
    urls: list[str] = []
    in_sources = False
    for line in markdown.splitlines():
        if line.strip() in SOURCES_HEADINGS:
            in_sources = True
            continue
        if in_sources and line.startswith("## "):
            break
        if not in_sources:
            continue
        for url in URL_RE.findall(line):
            cleaned = url.rstrip(".,>")
            if cleaned not in urls:
                urls.append(cleaned)
    return urls


def repo_relative_path(path: Path) -> str:
    """Return a repository-relative display path for a CLI path argument."""
    return str(path.resolve().relative_to(ROOT))


def decode_title(raw: bytes) -> str | None:
    match = TITLE_RE.search(raw)
    if not match:
        return None
    title = re.sub(rb"\s+", b" ", match.group(1)).strip()
    try:
        return html.unescape(title.decode("utf-8", errors="replace"))
    except UnicodeDecodeError:
        return html.unescape(title.decode(errors="replace"))


def open_url(url: str, timeout: float, method: str) -> urllib.response.addinfourl:
    request = urllib.request.Request(
        url,
        method=method,
        headers={
            "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7) AppleWebKit/537.36 "
            "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 mh-save-sync-ui-research-link-check/0.1",
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        },
    )
    context = ssl.create_default_context()
    return urllib.request.urlopen(request, timeout=timeout, context=context)


def fetch(url: str, timeout: float) -> dict[str, object]:
    head_status: int | None = None
    head_final_url: str | None = None
    head_error: str | None = None
    get_status: int | None = None
    get_error: str | None = None
    title: str | None = None
    bytes_sampled = 0

    try:
        with open_url(url, timeout, "HEAD") as response:
            head_status = response.getcode()
            head_final_url = response.geturl()
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, socket.timeout) as exc:
        head_status = getattr(exc, "code", None)
        head_final_url = getattr(exc, "url", None)
        head_error = f"{exc.__class__.__name__}: {exc}"

    try:
        with open_url(url, timeout, "GET") as response:
            get_status = response.getcode()
            final_url = response.geturl()
            body = response.read(MAX_READ_BYTES)
            bytes_sampled = len(body)
            title = decode_title(body)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, socket.timeout) as exc:
        get_status = getattr(exc, "code", None)
        get_error = f"{exc.__class__.__name__}: {exc}"
        # Avoid writing rate-limit/challenge redirect URLs into local artifacts.
        final_url = head_final_url or url

    if head_status is not None and head_status < 400:
        return {
            "url": url,
            "final_url": final_url,
            "head_status": head_status,
            "get_status": get_status,
            "head_error": head_error,
            "title": title,
            "bytes_sampled": bytes_sampled,
            "get_error": get_error,
            "reachable_via_head": True,
        }
    if get_status is not None and get_status < 400:
        return {
            "url": url,
            "final_url": final_url,
            "head_status": head_status,
            "get_status": get_status,
            "head_error": head_error,
            "title": title,
            "bytes_sampled": bytes_sampled,
            "get_error": get_error,
            "reachable_via_head": False,
        }
    raise urllib.error.URLError(
        f"unreachable: HEAD={head_status} {head_error or ''} GET={get_status} {get_error or ''}"
    )


def verify(urls: Iterable[str], timeout: float) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    ok: list[dict[str, object]] = []
    failed: list[dict[str, object]] = []
    for url in urls:
        try:
            result = fetch(url, timeout)
            ok.append(result)
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, socket.timeout) as exc:
            status = getattr(exc, "code", None)
            failed.append(
                {
                    "url": url,
                    "status": status,
                    "error": f"{exc.__class__.__name__}: {exc}",
                }
            )
    return ok, failed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()

    doc = args.doc.resolve()
    output = args.output.resolve()
    markdown = doc.read_text(encoding="utf-8")
    urls = extract_source_urls(markdown)
    if not urls:
        print(f"no source URLs found in {doc}", file=sys.stderr)
        return 2

    ok, failed = verify(urls, args.timeout)
    report = {
        "ui_ux_research_link_check": not failed,
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "doc": repo_relative_path(doc),
        "source_count": len(urls),
        "ok_count": len(ok),
        "failed_count": len(failed),
        "ok": ok,
        "failed": failed,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
