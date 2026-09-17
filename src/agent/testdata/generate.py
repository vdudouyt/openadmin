#!/usr/bin/env python3
"""Rebuild the archive fixtures `archive.rs` tests against.

Every archive is written by a real producer — CPython's `zipfile` and `tarfile`,
GNU tar, gzip — and never by OpenAdmin's own code: a reader tested only against
archives it wrote itself agrees with itself and nothing else. Timestamps are
fixed, so running this again produces the same bytes.

    python3 src/agent/testdata/generate.py
"""

import gzip
import io
import os
import shutil
import stat
import subprocess
import tarfile
import tempfile
import time
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
MTIME = 1_700_000_000
ENV = dict(os.environ, LC_ALL="C.UTF-8", TZ="UTC")

# (name, kind, mode, data, link target) — the tree sample.zip and sample.tar.gz share.
ENTRIES = [
    ("app-1.2.3/", "dir", 0o755, b"", None),
    ("app-1.2.3/bin/", "dir", 0o755, b"", None),
    ("app-1.2.3/bin/app", "file", 0o755, b"#!/bin/sh\necho app\n" * 40, None),
    ("app-1.2.3/install.sh", "file", 0o755, b"#!/bin/sh\nset -e\ncp bin/app /usr/local/bin/\n", None),
    ("app-1.2.3/etc/app.conf", "file", 0o644, b"listen = 8080\nworkers = 4\n", None),
    ("app-1.2.3/lib/libfoo.so", "link", 0o777, b"", "libfoo.so.1"),
    ("app-1.2.3/lib/libfoo.so.1", "file", 0o644, b"\x7fELF" + b"\x00" * 60, None),
    ("app-1.2.3/doc/résumé.txt", "file", 0o644, "non-ASCII name\n".encode(), None),
    ("app-1.2.3/bad\x1b[2Jname", "file", 0o644, b"control character in the name\n", None),
    ("../evil", "file", 0o644, b"escapes upward\n", None),
    ("/etc/cron.d/evil", "file", 0o644, b"escapes to root\n", None),
]


def zip_info(name):
    return zipfile.ZipInfo(name, date_time=time.gmtime(MTIME)[:6])


def sample_zip():
    """Deflate-compressed, unix attributes, a symlink stored uncompressed."""
    with zipfile.ZipFile(f"{HERE}/sample.zip", "w", compression=zipfile.ZIP_DEFLATED) as z:
        for name, kind, mode, data, link in ENTRIES:
            zi = zip_info(name)
            zi.create_system = 3
            if kind == "dir":
                zi.external_attr = (stat.S_IFDIR | mode) << 16 | 0x10
                z.writestr(zi, b"")
            elif kind == "link":
                zi.external_attr = (stat.S_IFLNK | mode) << 16
                z.writestr(zi, link.encode())
            else:
                zi.external_attr = (stat.S_IFREG | mode) << 16
                zi.compress_type = zipfile.ZIP_DEFLATED
                z.writestr(zi, data)


def sample_tar_gz():
    """The same tree as a GNU-format tar, plus a hardlink."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.GNU_FORMAT) as t:
        for name, kind, mode, data, link in ENTRIES:
            ti = tarfile.TarInfo(name)
            ti.mtime, ti.mode, ti.uname, ti.gname = MTIME, mode, "root", "root"
            if kind == "dir":
                ti.type = tarfile.DIRTYPE
                t.addfile(ti)
            elif kind == "link":
                ti.type, ti.linkname = tarfile.SYMTYPE, link
                t.addfile(ti)
            else:
                ti.size = len(data)
                t.addfile(ti, io.BytesIO(data))
        hl = tarfile.TarInfo("app-1.2.3/bin/app-alias")
        hl.mtime, hl.mode, hl.type, hl.linkname = MTIME, 0o755, tarfile.LNKTYPE, "app-1.2.3/bin/app"
        t.addfile(hl)
    with open(f"{HERE}/sample.tar.gz", "wb") as f:
        f.write(gzip.compress(buf.getvalue(), mtime=0))


def decoys():
    """Files whose names say one format and whose bytes say another."""
    with open(f"{HERE}/gzip-named.zip", "wb") as f:
        f.write(gzip.compress(b"not a zip", mtime=0))
    shutil.copy(f"{HERE}/sample.zip", f"{HERE}/zip-named.tar.gz")


def multimember_tar_gz():
    """One tar stream from GNU tar, cut at a block boundary and gzipped as two
    members, then concatenated — what `cat a.gz b.gz` or a parallel compressor
    produces. The escaping entry is in the *second* member, so a reader that
    stops after the first member cannot pass by accident."""
    work = tempfile.mkdtemp()
    try:
        os.makedirs(f"{work}/pkg/etc")
        for rel, body in [("pkg/install.sh", "#!/bin/sh\n"), ("pkg/etc/app.conf", "a = 1\n")]:
            with open(f"{work}/{rel}", "w") as fh:
                fh.write(body)
        os.chmod(f"{work}/pkg/install.sh", 0o755)
        with open(f"{work}/evil", "w") as fh:
            fh.write("x\n")
        tar = subprocess.run(
            ["tar", "--format=gnu", "--owner=0", "--group=0", "--mtime=@%d" % MTIME,
             "--sort=name", "-cf", "-", "pkg",
             "--transform=s,^evil$,../evil,", "--absolute-names", "evil"],
            cwd=work, env=ENV, check=True, capture_output=True,
        ).stdout
        # Four headers and data blocks in: the second member starts mid-archive.
        cut = 512 * 4
        with open(f"{HERE}/multimember.tar.gz", "wb") as f:
            f.write(gzip.compress(tar[:cut], mtime=0))
            f.write(gzip.compress(tar[cut:], mtime=0))
    finally:
        shutil.rmtree(work)


def deflated_symlink_zip():
    """CPython compresses a symlink entry like any other when the archive is
    deflated, so the target has to be inflated to be read."""
    with zipfile.ZipFile(f"{HERE}/deflated-link.zip", "w", compression=zipfile.ZIP_DEFLATED) as z:
        zi = zip_info("pkg/lib/libfoo.so")
        zi.create_system = 3
        zi.external_attr = (stat.S_IFLNK | 0o777) << 16
        zi.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(zi, b"libfoo.so.1")
    with zipfile.ZipFile(f"{HERE}/deflated-link.zip") as z:
        assert z.getinfo("pkg/lib/libfoo.so").compress_type == zipfile.ZIP_DEFLATED


def dos_zip():
    """As Windows writes one: MS-DOS host, DOS attributes only, no unix mode
    bits stored at all."""
    with zipfile.ZipFile(f"{HERE}/dos.zip", "w", compression=zipfile.ZIP_DEFLATED) as z:
        for name, attr, data in [("docs/", 0x10, b""), ("docs/setup.exe", 0x20, b"MZ")]:
            zi = zip_info(name)
            zi.create_system = 0
            zi.external_attr = attr
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, data)
    with zipfile.ZipFile(f"{HERE}/dos.zip") as z:
        assert all(i.external_attr >> 16 == 0 for i in z.infolist()), "no unix bits"


if __name__ == "__main__":
    sample_zip()
    sample_tar_gz()
    decoys()
    multimember_tar_gz()
    deflated_symlink_zip()
    dos_zip()
    print("fixtures written to", HERE)
