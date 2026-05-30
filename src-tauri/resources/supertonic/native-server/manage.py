from __future__ import annotations

import argparse
import hashlib
import os
import platform
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[1]
NATIVE_DIR = ROOT_DIR / "native-server"
VENV_DIR = ROOT_DIR / ".native-venv"
RUNTIME_DIR = ROOT_DIR / "native-runtime"
MODEL_CACHE_DIR = RUNTIME_DIR / "model-cache"
LOG_FILE = RUNTIME_DIR / "server.log"
PID_FILE = RUNTIME_DIR / "server.pid"
REQ_FILE = NATIVE_DIR / "requirements.txt"
REQ_HASH_FILE = RUNTIME_DIR / "requirements.sha256"
CONFIG_FILE = NATIVE_DIR / "config.env"


def load_config() -> dict[str, str]:
    config = {
        "HOST": "127.0.0.1",
        "PORT": "7788",
        "MODEL": "supertonic-3",
        "LOG_LEVEL": "info",
        "CORS": "http://localhost:*,http://127.0.0.1:*",
        "SUPERTONIC_INTRA_OP_THREADS": "8",
        "SUPERTONIC_INTER_OP_THREADS": "1",
        "OMP_NUM_THREADS": "8",
        "OPENBLAS_NUM_THREADS": "8",
        "MKL_NUM_THREADS": "8",
    }
    if CONFIG_FILE.exists():
        for raw_line in CONFIG_FILE.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            config[key.strip()] = value.strip().strip('"').strip("'")

    for key in list(config):
        if os.environ.get(key):
            config[key] = os.environ[key]
    return config


def venv_python() -> Path:
    if platform.system() == "Windows":
        return VENV_DIR / "Scripts" / "python.exe"
    return VENV_DIR / "bin" / "python"


def venv_supertonic() -> Path:
    if platform.system() == "Windows":
        return VENV_DIR / "Scripts" / "supertonic.exe"
    return VENV_DIR / "bin" / "supertonic"


def requirements_hash() -> str:
    return hashlib.sha256(REQ_FILE.read_bytes()).hexdigest()


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    print("+ " + " ".join(cmd))
    return subprocess.run(cmd, check=True, **kwargs)


def ensure_runtime_dirs() -> None:
    RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
    MODEL_CACHE_DIR.mkdir(parents=True, exist_ok=True)


def ensure_venv() -> None:
    ensure_runtime_dirs()
    py = venv_python()
    if not py.exists():
        print(f"Creating native Python environment: {VENV_DIR}")
        run([sys.executable, "-m", "venv", str(VENV_DIR)])

    current_hash = requirements_hash()
    installed_hash = REQ_HASH_FILE.read_text(encoding="utf-8").strip() if REQ_HASH_FILE.exists() else ""
    if installed_hash != current_hash:
        print("Installing native Supertonic dependencies...")
        run([str(py), "-m", "pip", "install", "--upgrade", "pip"])
        run([str(py), "-m", "pip", "install", "-r", str(REQ_FILE)])
        REQ_HASH_FILE.write_text(current_hash, encoding="utf-8")


def read_pid() -> int | None:
    if not PID_FILE.exists():
        return None
    try:
        return int(PID_FILE.read_text(encoding="utf-8").strip())
    except ValueError:
        return None


def process_running(pid: int | None) -> bool:
    if not pid:
        return False
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def port_is_open(host: str, port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.4)
        return sock.connect_ex((host, port)) == 0


def health_url(config: dict[str, str]) -> str:
    return f"http://{config['HOST']}:{int(config['PORT'])}/v1/health"


def wait_for_health(config: dict[str, str], timeout_s: int = 240) -> bool:
    deadline = time.time() + timeout_s
    url = health_url(config)
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=3) as response:
                if response.status == 200:
                    return True
        except (urllib.error.URLError, TimeoutError):
            pass
        time.sleep(2)
    return False


def server_env(config: dict[str, str]) -> dict[str, str]:
    env = os.environ.copy()
    env.update(config)
    env["SUPERTONIC_CACHE_DIR"] = str(MODEL_CACHE_DIR)
    env.setdefault("PYTHONUTF8", "1")
    return env


def start_server(wait: bool = True) -> int:
    config = load_config()
    ensure_runtime_dirs()

    existing_pid = read_pid()
    if process_running(existing_pid):
        print(f"Supertonic native server already running with PID {existing_pid}.")
        print(f"Health: {health_url(config)}")
        return 0

    host = config["HOST"]
    port = int(config["PORT"])
    if port_is_open(host, port):
        print(f"Port {host}:{port} is already in use.")
        print("Stop the other service first, or edit native-server/config.env.")
        return 2

    ensure_venv()
    command = [
        str(venv_supertonic()),
        "serve",
        "--host",
        host,
        "--port",
        str(port),
        "--model",
        config["MODEL"],
        "--log-level",
        config["LOG_LEVEL"],
    ]
    if config.get("CORS"):
        command.extend(["--cors", config["CORS"]])

    log_handle = LOG_FILE.open("ab")
    popen_kwargs: dict[str, object] = {
        "cwd": str(ROOT_DIR),
        "stdout": log_handle,
        "stderr": subprocess.STDOUT,
        "env": server_env(config),
    }
    if platform.system() == "Windows":
        popen_kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP  # type: ignore[attr-defined]
    else:
        popen_kwargs["start_new_session"] = True

    process = subprocess.Popen(command, **popen_kwargs)
    PID_FILE.write_text(str(process.pid), encoding="utf-8")
    print(f"Started Supertonic native server with PID {process.pid}.")
    print(f"Log: {LOG_FILE}")
    print(f"Health: {health_url(config)}")

    if wait:
        print("Waiting for server readiness. First run may download model files.")
        if wait_for_health(config):
            print("Supertonic native server is ready.")
            return 0
        print("Server did not become ready before timeout. Check the log file.")
        return 3
    return 0


def stop_server() -> int:
    pid = read_pid()
    if not process_running(pid):
        if PID_FILE.exists():
            PID_FILE.unlink()
        print("Supertonic native server is not running.")
        return 0

    assert pid is not None
    print(f"Stopping Supertonic native server PID {pid}...")
    if platform.system() == "Windows":
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], check=False)
    else:
        try:
            os.killpg(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        deadline = time.time() + 10
        while time.time() < deadline and process_running(pid):
            time.sleep(0.25)
        if process_running(pid):
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    if PID_FILE.exists():
        PID_FILE.unlink()
    print("Stopped.")
    return 0


def status() -> int:
    config = load_config()
    pid = read_pid()
    running = process_running(pid)
    print(f"PID file: {PID_FILE if PID_FILE.exists() else 'none'}")
    print(f"Process: {'running' if running else 'not running'}" + (f" ({pid})" if pid else ""))
    print(f"URL: http://{config['HOST']}:{int(config['PORT'])}")
    print(f"Health: {health_url(config)}")
    print(f"Log: {LOG_FILE}")
    return 0 if running else 1


def test_speech() -> int:
    config = load_config()
    out = RUNTIME_DIR / "test.wav"
    payload = (
        b'{'
        b'"text":"Supertonic native Python server is speaking locally.",'
        b'"voice":"M1","lang":"en","steps":4,"speed":1.05,"response_format":"wav"'
        b"}"
    )
    req = urllib.request.Request(
        f"http://{config['HOST']}:{int(config['PORT'])}/v1/tts",
        data=payload,
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=180) as response:
        out.write_bytes(response.read())
    print(f"Wrote {out}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Manage the native Supertonic local server.")
    sub = parser.add_subparsers(dest="command", required=True)
    start = sub.add_parser("start")
    start.add_argument("--no-wait", action="store_true", help="Do not wait for /v1/health.")
    sub.add_parser("stop")
    sub.add_parser("status")
    sub.add_parser("test")
    args = parser.parse_args()

    if args.command == "start":
        return start_server(wait=not args.no_wait)
    if args.command == "stop":
        return stop_server()
    if args.command == "status":
        return status()
    if args.command == "test":
        return test_speech()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
