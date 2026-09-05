#!/usr/bin/env python3
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
lock = json.loads((ROOT / "integration/dependencies.lock.json").read_text(encoding="utf-8"))
assert lock["schema"] == "happy-wakey.backend-integrations.v1"
expected = {"ores-middleware", "ores-rate-limit", "ores-otel", "opto-sync", "ores-chat", "shared-auth"}
actual = {item["name"] for item in lock["dependencies"]}
assert actual == expected
for item in lock["dependencies"]:
    assert re.fullmatch(r"[a-f0-9]{40}", item["revision"])
    assert item["url"].startswith("https://github.com/")
    assert "token" not in item["url"].lower()
source = (ROOT / "src/lib.rs").read_text(encoding="utf-8")
for forbidden in ("SUPABASE_SERVICE_ROLE", "DATABASE_URL=", "Authorization: Bearer", "linkedin.com/feed"):
    assert forbidden not in source
print(f"validated {len(actual)} immutable backend integrations and source safety")
