"""Set a public endpoint only; an empty variable leaves password login disabled."""
import json
import os
import re
from pathlib import Path
from urllib.parse import urlparse

value = os.environ.get('AUTH_URL', '')
if value:
    url = urlparse(value)
    if url.scheme != 'https' or not url.hostname or url.username or url.password or url.path not in ('', '/') or url.query or url.fragment:
        raise SystemExit('AUTH_URL must be an HTTPS origin')
    value = value.rstrip('/')
p = Path('lit-static/dapps/dashboard/password-client.js')
s = p.read_text()
pattern = r"([\"'])__LIT_AUTH_BASE_URL__\1"
updated, count = re.subn(pattern, lambda _: json.dumps(value), s)
if count != 1:
    raise SystemExit('Expected exactly one password auth placeholder')
p.write_text(updated)
