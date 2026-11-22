#!/usr/bin/env python3
"""Regenerates crates/infrastructure/tests/fixtures/sample.pdf (no dependencies).

Three pages, a large heading, body text, non-ASCII text (WinAnsi encoded) and a
bookmark per chapter. Original content, nothing copyrighted.
Usage: python3 scripts/gen-fixture-pdf.py crates/infrastructure/tests/fixtures/sample.pdf
"""
import sys

PAGES = [
    ("Capitolo 1: Introduzione", ["Questo e' un libro di prova.", "Perch\xe9 testare con un PDF vero? Per vedere PDFium all'opera."]),
    ("Capitolo 2: Ownership", ["Ogni valore ha un solo proprietario.", "La citt\xe0 \xe8 pi\xf9 grande del paese."]),
    ("Capitolo 3: Conclusione", ["Fine del libro di prova."]),
]


def esc(s):
    b = s.encode("cp1252")
    return b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


def main(out):
    objs = {}
    n = len(PAGES)
    # 1 catalog, 2 pages, 3 font regular, 4 font bold, then page/content pairs, then outlines.
    page = lambda i: 5 + 2 * i
    content = lambda i: 6 + 2 * i
    root = 5 + 2 * n
    item = lambda i: root + 1 + i
    objs[1] = b"<< /Type /Catalog /Pages 2 0 R /Outlines %d 0 R /PageMode /UseOutlines >>" % root
    kids = b" ".join(b"%d 0 R" % page(i) for i in range(n))
    objs[2] = b"<< /Type /Pages /Kids [" + kids + b"] /Count %d >>" % n
    objs[3] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
    objs[4] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
    for i, (head, lines) in enumerate(PAGES):
        stream = b"BT /F2 24 Tf 72 740 Td (" + esc(head) + b") Tj ET\n"
        y = 690
        for line in lines:
            stream += b"BT /F1 12 Tf 72 %d Td (" % y + esc(line) + b") Tj ET\n"
            y -= 20
        objs[page(i)] = (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
                         b"/Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>" % content(i))
        objs[content(i)] = b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"endstream"
    objs[root] = b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count %d >>" % (item(0), item(n - 1), n)
    for i, (head, _) in enumerate(PAGES):
        d = b"<< /Title (" + esc(head) + b") /Parent %d 0 R /Dest [%d 0 R /Fit]" % (root, page(i))
        if i > 0:
            d += b" /Prev %d 0 R" % item(i - 1)
        if i < n - 1:
            d += b" /Next %d 0 R" % item(i + 1)
        objs[item(i)] = d + b" >>"
    out_b = b"%PDF-1.4\n"
    offsets = {}
    for k in sorted(objs):
        offsets[k] = len(out_b)
        out_b += b"%d 0 obj\n" % k + objs[k] + b"\nendobj\n"
    xref = len(out_b)
    total = max(objs) + 1
    out_b += b"xref\n0 %d\n0000000000 65535 f \n" % total
    for k in range(1, total):
        out_b += b"%010d 00000 n \n" % offsets[k]
    out_b += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (total, xref)
    open(out, "wb").write(out_b)


main(sys.argv[1])
