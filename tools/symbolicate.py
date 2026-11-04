#!/usr/bin/env python3
"""Names the frames of a Cold Open crash report (docs/specs/crash-reports.md §5).

A report's frames are offsets into the build's modules (`Cold Open.exe+0x1a2b`).
This reads the symbols of the same build and prints each frame of Cold Open's
own program with its function, file and line.

Usage:
  symbolicate.py REPORT SYMBOLS

SYMBOLS is a folder of symbols for the report's build: a release's
ColdOpen-<version>-symbols.zip unzipped (one folder per target), one target's
folder from it, or a developer's own target/<profile> folder.

It uses llvm-symbolizer when it is on PATH (any OS, any target). Otherwise
it uses the tool of the OS the report came from, which must be this one:
addr2line on Linux, atos on macOS, Windows' own dbghelp.dll for a PDB.
"""

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

# The module names of the program a report can come from.
OURS = {"Cold Open.exe", "coldopen-app.exe", "coldopen-app", "coldopen-driver.exe", "coldopen-driver"}
FRAME = re.compile(r"^\s*(\d+): (.+)\+0x([0-9a-f]+)$")
# The base address a macOS executable's __TEXT segment asks for.
MACHO_BASE = 0x1_0000_0000


def read_report(path):
    target, frames = None, []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.startswith("Target: "):
            target = line[len("Target: "):].strip()
        m = FRAME.match(line)
        if m:
            frames.append((int(m.group(1)), m.group(2), int(m.group(3), 16)))
    if not target:
        sys.exit(f"{path}: no Target line; is it a crash report?")
    return target, frames


def program_of(frames):
    """The program the report came from: the app, or the tests' driver."""
    names = {m for _, m, _ in frames}
    return "coldopen-driver" if names & {"coldopen-driver", "coldopen-driver.exe"} else "coldopen-app"


def find_symbols(folder, target, program):
    """The symbols file of `program` for `target` under `folder`, and its kind."""
    folder = Path(folder)
    under = program.replace("-", "_")
    for base in (folder / target, folder):
        for name, kind in (
            (f"{program}.debug", "elf"),
            (f"{program}.dSYM", "macho"),
            (f"{under}.pdb", "pdb"),
        ):
            p = base / name
            if p.exists():
                return p, kind
        # A developer's own unstripped build.
        p = base / program
        if p.is_file():
            return p, "macho" if "apple" in target else "elf"
    sys.exit(f"no symbols of {program} for {target} under {folder}")


def macho_dwarf(dsym):
    dwarf = Path(dsym) / "Contents" / "Resources" / "DWARF"
    files = list(dwarf.iterdir()) if dwarf.is_dir() else []
    return files[0] if files else Path(dsym)


def with_llvm(symbols, kind, offsets):
    tool = shutil.which("llvm-symbolizer")
    if not tool:
        return None
    args = [tool, "--functions=linkage", "--demangle", "--inlining=false"]
    if kind == "pdb":
        exe = Path(symbols).with_name(Path(symbols).stem.replace("_", "-") + ".exe")
        args += ["--relative-address", f"--obj={exe if exe.is_file() else symbols}"]
        addrs = offsets
    elif kind == "macho":
        args.append(f"--obj={macho_dwarf(symbols)}")
        addrs = [MACHO_BASE + o for o in offsets]
    else:
        args.append(f"--obj={symbols}")
        addrs = offsets
    out = subprocess.run(args + [hex(a) for a in addrs], capture_output=True, text=True, check=True)
    # Two lines per address, then a blank line.
    blocks = [b.splitlines() for b in out.stdout.strip("\n").split("\n\n")]
    return [(b[0], b[1] if len(b) > 1 else "") for b in blocks]


def with_addr2line(symbols, offsets):
    out = subprocess.run(
        ["addr2line", "-f", "-C", "-e", str(symbols)] + [hex(o) for o in offsets],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()
    return [(out[i], out[i + 1]) for i in range(0, len(out) - 1, 2)]


def with_atos(symbols, offsets):
    out = subprocess.run(
        ["atos", "-o", str(macho_dwarf(symbols)), "-l", hex(MACHO_BASE)]
        + [hex(MACHO_BASE + o) for o in offsets],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()
    return [(line, "") for line in out]


def with_dbghelp(pdb, offsets):
    import ctypes
    from ctypes import wintypes

    class SYMBOL_INFOW(ctypes.Structure):
        _fields_ = [
            ("SizeOfStruct", wintypes.ULONG), ("TypeIndex", wintypes.ULONG),
            ("Reserved", ctypes.c_uint64 * 2), ("Index", wintypes.ULONG),
            ("Size", wintypes.ULONG), ("ModBase", ctypes.c_uint64),
            ("Flags", wintypes.ULONG), ("Value", ctypes.c_uint64),
            ("Address", ctypes.c_uint64), ("Register", wintypes.ULONG),
            ("Scope", wintypes.ULONG), ("Tag", wintypes.ULONG),
            ("NameLen", wintypes.ULONG), ("MaxNameLen", wintypes.ULONG),
            ("Name", ctypes.c_wchar * 1),
        ]

    class IMAGEHLP_LINEW64(ctypes.Structure):
        _fields_ = [
            ("SizeOfStruct", wintypes.DWORD), ("Key", ctypes.c_void_p),
            ("LineNumber", wintypes.DWORD), ("FileName", ctypes.c_wchar_p),
            ("Address", ctypes.c_uint64),
        ]

    dbghelp = ctypes.WinDLL("dbghelp.dll")
    dbghelp.SymInitializeW.argtypes = [wintypes.HANDLE, wintypes.LPCWSTR, wintypes.BOOL]
    dbghelp.SymLoadModuleExW.argtypes = [
        wintypes.HANDLE, wintypes.HANDLE, wintypes.LPCWSTR, wintypes.LPCWSTR,
        ctypes.c_uint64, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD,
    ]
    dbghelp.SymLoadModuleExW.restype = ctypes.c_uint64
    dbghelp.SymFromAddrW.argtypes = [wintypes.HANDLE, ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64), ctypes.c_void_p]
    dbghelp.SymGetLineFromAddrW64.argtypes = [wintypes.HANDLE, ctypes.c_uint64, ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(IMAGEHLP_LINEW64)]

    # SYMOPT_UNDNAME | SYMOPT_LOAD_LINES | SYMOPT_FAIL_CRITICAL_ERRORS
    dbghelp.SymSetOptions(0x2 | 0x10 | 0x200)
    process = wintypes.HANDLE(0x5EED)  # any handle not of a live process
    if not dbghelp.SymInitializeW(process, str(Path(pdb).parent), False):
        sys.exit(f"SymInitialize failed: {ctypes.GetLastError()}")
    # The exe beside the PDB, when it's there: dbghelp needs its section
    # table to turn an offset into a place in the PDB.
    exe = Path(pdb).with_name(Path(pdb).stem.replace("_", "-") + ".exe")
    image = exe if exe.is_file() else Path(pdb)
    base = 0x1000_0000
    if not dbghelp.SymLoadModuleExW(process, None, str(image), None, base, 0, None, 0):
        sys.exit(f"couldn't load {image}: {ctypes.GetLastError()}")
    names = []
    for off in offsets:
        buf = ctypes.create_string_buffer(ctypes.sizeof(SYMBOL_INFOW) + 2 * 1024)
        info = ctypes.cast(buf, ctypes.POINTER(SYMBOL_INFOW)).contents
        info.SizeOfStruct = ctypes.sizeof(SYMBOL_INFOW)
        info.MaxNameLen = 1024
        disp = ctypes.c_uint64()
        name = "??"
        if dbghelp.SymFromAddrW(process, base + off, ctypes.byref(disp), buf):
            name = ctypes.wstring_at(ctypes.addressof(info) + SYMBOL_INFOW.Name.offset, info.NameLen)
        line = IMAGEHLP_LINEW64()
        line.SizeOfStruct = ctypes.sizeof(IMAGEHLP_LINEW64)
        d32 = wintypes.DWORD()
        where = ""
        if dbghelp.SymGetLineFromAddrW64(process, base + off, ctypes.byref(d32), ctypes.byref(line)):
            where = f"{line.FileName}:{line.LineNumber}"
        names.append((name, where))
    dbghelp.SymCleanup(process)
    return names


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    target, frames = read_report(sys.argv[1])
    symbols, kind = find_symbols(sys.argv[2], target, program_of(frames))
    ours = [(n, m, o) for n, m, o in frames if m in OURS]
    # A return address is just after its call; one byte back is inside it.
    offsets = [max(o - 1, 0) for _, _, o in ours]
    names = with_llvm(symbols, kind, offsets) if offsets else []
    if names is None:
        host = sys.platform
        if kind == "pdb" and host == "win32":
            names = with_dbghelp(symbols, offsets)
        elif kind == "elf" and shutil.which("addr2line"):
            names = with_addr2line(symbols, offsets)
        elif kind == "macho" and shutil.which("atos"):
            names = with_atos(symbols, offsets)
        else:
            sys.exit(f"no tool here reads {kind} symbols: install LLVM for llvm-symbolizer")
    resolved = {n: r for (n, _, _), r in zip(ours, names)}
    for n, module, off in frames:
        if n in resolved:
            name, where = resolved[n]
            print(f"{n:4}: {module}+0x{off:x}  {name}" + (f"  ({where})" if where else ""))
        else:
            print(f"{n:4}: {module}+0x{off:x}")


if __name__ == "__main__":
    main()
