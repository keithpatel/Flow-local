import os
import sys
import zipfile
import urllib.request
import hashlib

WHISPER_BIN_URL = "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip"
BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN_DIR = os.path.join(BASE_DIR, "bin", "whisper")
MODELS_DIR = os.path.join(BASE_DIR, "models")

os.makedirs(BIN_DIR, exist_ok=True)
os.makedirs(MODELS_DIR, exist_ok=True)

def download_with_progress(url, dest_path):
    print(f"Downloading {url} -> {dest_path}")
    def reporthook(count, block_size, total_size):
        if total_size > 0:
            percent = int(count * block_size * 100 / total_size)
            sys.stdout.write(f"\rDownloading: {percent}% ({count * block_size // (1024*1024)}MB / {total_size // (1024*1024)}MB)")
            sys.stdout.flush()
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    with urllib.request.urlopen(req) as resp, open(dest_path, 'wb') as out_file:
        total_size = int(resp.getheader('Content-Length', 0))
        downloaded = 0
        block_size = 64 * 1024
        while True:
            chunk = resp.read(block_size)
            if not chunk:
                break
            out_file.write(chunk)
            downloaded += len(chunk)
            if total_size > 0:
                percent = int(downloaded * 100 / total_size)
                sys.stdout.write(f"\rDownloading: {percent}% ({downloaded // (1024*1024)}MB / {total_size // (1024*1024)}MB)")
                sys.stdout.flush()
    print("\nDownload complete.")

def setup_whisper_bin():
    cli_path = os.path.join(BIN_DIR, "whisper-cli.exe")
    if os.path.exists(cli_path):
        print(f"Whisper binary already exists at {cli_path}")
        return
    zip_path = os.path.join(BIN_DIR, "whisper-bin-x64.zip")
    download_with_progress(WHISPER_BIN_URL, zip_path)
    print("Extracting whisper-bin-x64.zip...")
    with zipfile.ZipFile(zip_path, 'r') as zip_ref:
        zip_ref.extractall(BIN_DIR)
    os.remove(zip_path)
    print(f"Extracted whisper binaries to {BIN_DIR}")

def setup_model(model_name="tiny.en"):
    model_filename = f"ggml-{model_name}.bin"
    model_path = os.path.join(MODELS_DIR, model_filename)
    if os.path.exists(model_path):
        print(f"Model already exists at {model_path} ({os.path.getsize(model_path) // (1024*1024)} MB)")
        return model_path
    url = f"https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{model_filename}"
    download_with_progress(url, model_path)
    print(f"Model ready at {model_path}")
    return model_path

if __name__ == "__main__":
    setup_whisper_bin()
    setup_model("tiny.en")
