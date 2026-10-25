import urllib.request
import json

req = urllib.request.Request(
    'https://api.github.com/repos/ggerganov/whisper.cpp/releases',
    headers={'User-Agent': 'Mozilla/5.0'}
)
with urllib.request.urlopen(req) as resp:
    releases = json.loads(resp.read().decode('utf-8'))
    for rel in releases:
        assets = [a['name'] for a in rel.get('assets', [])]
        if assets:
            print(f"Tag with assets: {rel['tag_name']}")
            for a in rel.get('assets', []):
                print(f"   {a['name']}: {a['browser_download_url']}")
            break
