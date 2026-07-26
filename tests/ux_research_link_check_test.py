#!/usr/bin/env python3
"""Regression checks for the UI research source-link parser."""

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import unittest


REPO = Path(__file__).resolve().parents[1]
SCRIPT = REPO / "scripts" / "ux-research-link-check.py"
DOC = REPO / "docs" / "research" / "MH3G_CONVERTER_UI_OPTIONS.md"


def load_link_check_module():
    spec = spec_from_file_location("ux_research_link_check", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class UiResearchLinkCheckTest(unittest.TestCase):
    def test_relative_doc_path_is_reported_from_repo_root(self) -> None:
        module = load_link_check_module()

        self.assertEqual(
            module.repo_relative_path(
                Path("docs/research/MH3G_CONVERTER_UI_OPTIONS.md")
            ),
            "docs/research/MH3G_CONVERTER_UI_OPTIONS.md",
        )

    def test_extracts_the_three_mh3g_sources(self) -> None:
        module = load_link_check_module()
        urls = module.extract_source_urls(DOC.read_text(encoding="utf-8"))

        self.assertEqual(
            urls,
            [
                "https://v2.tauri.app/start/",
                "https://docs.flutter.dev/reference/supported-platforms",
                "https://learn.microsoft.com/windows/apps/winui/winui3/",
            ],
        )

    def test_keeps_sources_reviewed_table_support(self) -> None:
        module = load_link_check_module()
        markdown = "\n".join(
            [
                "## Sources reviewed",
                "| Source | URL |",
                "| --- | --- |",
                "| Tauri | https://v2.tauri.app/start/ |",
                "## Next",
            ]
        )

        self.assertEqual(
            module.extract_source_urls(markdown),
            ["https://v2.tauri.app/start/"],
        )

    def test_does_not_scan_outside_a_sources_section(self) -> None:
        module = load_link_check_module()
        markdown = "\n".join(
            [
                "https://outside.example.invalid/before",
                "## Sources",
                "- Source: <https://inside.example.invalid/source>",
                "## Next",
                "https://outside.example.invalid/after",
            ]
        )

        self.assertEqual(
            module.extract_source_urls(markdown),
            ["https://inside.example.invalid/source"],
        )

    def test_stops_at_a_second_level_heading_with_two_leading_spaces(self) -> None:
        module = load_link_check_module()
        markdown = "\n".join(
            [
                "## Sources",
                "- Source: <https://inside.example.invalid/source>",
                "  ## Next",
                "https://outside.example.invalid/after",
            ]
        )

        self.assertEqual(
            module.extract_source_urls(markdown),
            ["https://inside.example.invalid/source"],
        )


if __name__ == "__main__":
    unittest.main()
