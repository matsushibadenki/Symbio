#!/usr/bin/env python3
"""Download a pinned development model bundle and verify its SHA-256.
Usage: python3 scripts/setup-handwriting.py DESTINATION
Model weights are not redistributed by this repository. See docs/handwriting.md.
"""
from pathlib import Path
import hashlib
import json
import sys
import urllib.request

FILES={
 'encoder_int8.onnx':'272f17c7b120db61db6cd8d77e695d0139144044abd57d9b23284435a5d916fa',
 'decoder_int8.onnx':'658901d81ca3aa4b62b0da262e8fe67ebcfdac19cfb52ca2cfd076bebd481088',
 'vocab.json':'6ef75b588049981d2b5b2ceedcc7066dddcbb0c7d04ee2cbfb0ec861f192a850',
}
if len(sys.argv)!=2: raise SystemExit('usage: setup-handwriting.py DESTINATION')
folder=Path(sys.argv[1]).resolve();folder.mkdir(parents=True,exist_ok=True)
for name,digest in FILES.items():
 path=folder/name
 if path.exists(): data=path.read_bytes()
 else:
  with urllib.request.urlopen('https://github.com/kimseungdae/ink-on/releases/download/v0.1.0/'+name,timeout=60) as response: data=response.read(32*1024*1024+1)
 if len(data)>32*1024*1024 or hashlib.sha256(data).hexdigest()!=digest: raise SystemExit('model integrity mismatch: '+name)
 if not path.exists():
  with path.open('xb') as output: output.write(data)
(folder/'provenance.json').write_text(json.dumps({'source':'kimseungdae/ink-on','release':'v0.1.0','sha256':FILES},indent=2))
print(folder)
