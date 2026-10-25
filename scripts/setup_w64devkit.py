import os
import subprocess
import urllib.request

url = "https://github.com/skeeto/w64devkit/releases/download/v2.10.1/w64devkit-x64-2.10.1.7z.exe"
dest = os.path.join(os.environ.get('TEMP', '.'), "w64devkit.exe")
extract_dir = os.path.join(os.environ.get('LOCALAPPDATA', '.'), "w64devkit")

if not os.path.exists(os.path.join(extract_dir, "w64devkit", "bin", "gcc.exe")):
    print(f"Downloading w64devkit from {url}...")
    urllib.request.urlretrieve(url, dest)
    print("Extracting w64devkit...")
    os.makedirs(extract_dir, exist_ok=True)
    subprocess.run([dest, f"-o{extract_dir}", "-y"], check=True)
    print("w64devkit extracted successfully!")
else:
    print("w64devkit already present!")

bin_dir = os.path.join(extract_dir, "w64devkit", "bin")
print(f"Bin dir: {bin_dir}")
