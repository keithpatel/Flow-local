import urllib.request
import json
import zipfile
import os

req = urllib.request.Request(
    'https://api.github.com/repos/skeeto/w64devkit/releases/latest',
    headers={'User-Agent': 'Mozilla/5.0'}
)
with urllib.request.urlopen(req) as resp:
    data = json.loads(resp.read().decode('utf-8'))
    print("Tag:", data.get('tag_name'))
    for a in data.get('assets', []):
        name = a['name']
        url = a['browser_download_url']
        print(f"{name} -> {url}")
        if name.endswith('.zip'):
            dest = os.path.join(os.environ.get('TEMP', '.'), name)
            print(f"Downloading {name} ({a['size'] // (1024*1024)} MB)...")
            urllib.request.urlretrieve(url, dest)
            print("Extracting...")
            extract_dir = os.path.join(os.environ.get('LOCALAPPDATA', '.'), "w64devkit")
            with zipfile.ZipFile(dest, 'r') as z:
                z.extractall(extract_dir)
            print(f"w64devkit extracted to {extract_dir}")
            break
