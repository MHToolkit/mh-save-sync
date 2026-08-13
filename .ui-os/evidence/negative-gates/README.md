# Windows UIOS negative gate protocol

Each mutation is applied only in a temporary copy by
`scripts/test-mh3g-save-converter-windows-ui-negative-gates.py`. The real
candidate is never edited. A gate passes only when the verifier rejects:

1. removal of the primary Inspect `AutomationId`;
2. removal of the minimum-window contract marker;
3. an artifact whose bytes no longer match evidence metadata;
4. evidence bound to a different Git commit.

Runtime viewport geometry remains a native-Windows gate; the source mutation
proves only that the machine contract cannot silently lose the minimum cell.
