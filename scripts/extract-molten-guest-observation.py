"""Recover one complete packed observation from a Molten guest console capture.

Byte integrity is not a campaign verdict or source admission.
"""

import argparse
import re
import subprocess
from pathlib import Path


MAX_BYTES = 64 * 1024
CHUNK_BYTES = 512
HEADER = re.compile(rb"molten-observation-v1 ([0-9a-f]{64}) ([1-9][0-9]{0,4})")
CHUNK = b"molten-observation-data-v1 "
END = b"molten-observation-end-v1 "
HEX = re.compile(rb"[0-9a-f]+")


def extract(lines):
    digest = None
    size = 0
    data = bytearray()
    complete = False
    for raw in lines:
        line = raw.rstrip(b"\r\n")
        if line.startswith(b"molten-observation-v1 "):
            header = HEADER.fullmatch(line)
            if digest is not None or header is None:
                raise ValueError("duplicate or malformed observation header")
            digest, length = header.groups()
            size = int(length)
            if size > MAX_BYTES:
                raise ValueError("observation exceeds byte limit")
        elif line.startswith(CHUNK):
            text = line[len(CHUNK):]
            expected = min(CHUNK_BYTES, size - len(data)) * 2
            if digest is None or complete or len(text) != expected or not HEX.fullmatch(text):
                raise ValueError("unexpected or malformed observation chunk")
            data.extend(bytes.fromhex(text.decode("ascii")))
        elif line.startswith(END):
            if digest is None or complete or line[len(END):] != digest or len(data) != size:
                raise ValueError("unexpected or incomplete observation end")
            complete = True
    if not complete:
        raise ValueError("no complete observation")
    measured = subprocess.run(
        ["b3sum", "--no-names"], input=data, capture_output=True, check=True,
    ).stdout.strip()
    if measured != digest:
        raise ValueError("observation digest mismatch")
    return digest.decode("ascii"), data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("serial_log", type=Path)
    parser.add_argument("output_directory", type=Path)
    args = parser.parse_args()
    try:
        with args.serial_log.open("rb") as source:
            digest, data = extract(source)
        args.output_directory.mkdir(parents=True, exist_ok=True)
        with (args.output_directory / f"{digest}.preserves").open("xb") as output:
            output.write(data)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"molten guest observation: {error}\n")
    print(f"blake3:{digest} bytes={len(data)}")


if __name__ == "__main__":
    main()
