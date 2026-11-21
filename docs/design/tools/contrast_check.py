#!/usr/bin/env python3
"""Verifica i rapporti di contrasto WCAG di docs/design/tokens.json (ruoli colore custom v2).

Uso:  python3 -I docs/design/tools/contrast_check.py
Esce con codice 1 se una coppia non raggiunge il minimo. Rieseguire a ogni modifica dei colori.
Le coppie (testo/elemento, sfondo, minimo, uso) sono in PAIRS: ogni coppia usata dall'app va aggiunta.
"""
import json, pathlib, sys
def lum(h):
    h=h.lstrip('#'); r,g,b=[int(h[i:i+2],16)/255 for i in (0,2,4)]
    f=lambda c: c/12.92 if c<=0.03928 else ((c+0.055)/1.055)**2.4
    return 0.2126*f(r)+0.7152*f(g)+0.0722*f(b)
def cr(a,b):
    la,lb=lum(a),lum(b)
    if la<lb: la,lb=lb,la
    return (la+0.05)/(lb+0.05)
PAIRS = [
    ('ink', 'bg', 7, 'Testo principale su sfondo app (AAA)'),
    ('ink', 'surface', 7, 'Testo principale su card (AAA)'),
    ('ink', 'surfaceSunken', 7, 'Testo principale su campi e aree incassate (AAA)'),
    ('ink', 'surfaceRaised', 7, 'Testo principale su dialog / sheet (AAA)'),
    ('ink', 'surfacePressed', 7, 'Testo principale su riga/superficie premuta (AAA)'),
    ('ink', 'accentSoft', 7, 'Testo principale su selezione (AAA)'),
    ('ink', 'accentSoftPressed', 7, 'Testo principale su selezione premuta (AAA)'),
    ('inkSoft', 'bg', 4.5, 'Testo secondario su bg'),
    ('inkMuted', 'bg', 4.5, 'Placeholder / meta-dati su bg'),
    ('accent', 'bg', 4.5, 'Link / testo accento su bg'),
    ('knowledge', 'bg', 4.5, 'Termini del glossario / fonti su bg'),
    ('border', 'bg', 3, 'Bordo dei controlli su bg (non testo)'),
    ('focus', 'bg', 3, 'Anello di focus su bg'),
    ('inkSoft', 'surface', 4.5, 'Testo secondario su surface'),
    ('inkMuted', 'surface', 4.5, 'Placeholder / meta-dati su surface'),
    ('accent', 'surface', 4.5, 'Link / testo accento su surface'),
    ('knowledge', 'surface', 4.5, 'Termini del glossario / fonti su surface'),
    ('border', 'surface', 3, 'Bordo dei controlli su surface (non testo)'),
    ('focus', 'surface', 3, 'Anello di focus su surface'),
    ('inkSoft', 'surfaceSunken', 4.5, 'Testo secondario su surfaceSunken'),
    ('inkMuted', 'surfaceSunken', 4.5, 'Placeholder / meta-dati su surfaceSunken'),
    ('accent', 'surfaceSunken', 4.5, 'Link / testo accento su surfaceSunken'),
    ('knowledge', 'surfaceSunken', 4.5, 'Termini del glossario / fonti su surfaceSunken'),
    ('border', 'surfaceSunken', 3, 'Bordo dei controlli su surfaceSunken (non testo)'),
    ('focus', 'surfaceSunken', 3, 'Anello di focus su surfaceSunken'),
    ('inkSoft', 'surfaceRaised', 4.5, 'Testo secondario su surfaceRaised'),
    ('inkMuted', 'surfaceRaised', 4.5, 'Placeholder / meta-dati su surfaceRaised'),
    ('accent', 'surfaceRaised', 4.5, 'Link / testo accento su surfaceRaised'),
    ('knowledge', 'surfaceRaised', 4.5, 'Termini del glossario / fonti su surfaceRaised'),
    ('border', 'surfaceRaised', 3, 'Bordo dei controlli su surfaceRaised (non testo)'),
    ('focus', 'surfaceRaised', 3, 'Anello di focus su surfaceRaised'),
    ('accent', 'accentSoft', 4.5, 'Accento su selezione'),
    ('danger', 'surfaceRaised', 4.5, 'Azione distruttiva nel dialog'),
    ('danger', 'surfaceSunken', 4.5, 'Errore su campo incassato'),
    ('danger', 'surfacePressed', 4.5, 'Azione distruttiva premuta'),
    ('focus', 'accentSoft', 3, 'Focus su selezione'),
    ('onAccent', 'accent', 4.5, 'Testo del bottone primario'),
    ('onAccent', 'accentPressed', 4.5, 'Bottone primario premuto'),
    ('accent', 'surfacePressed', 4.5, 'Bottone secondario/testuale premuto'),
    ('ink', 'surfacePressed', 7, 'Testo su stato premuto'),
    ('knowledge', 'knowledgeSoft', 4.5, 'Termine su sfondo conoscenza'),
    ('ink', 'knowledgeSoft', 7, 'Testo su sfondo conoscenza'),
    ('correct', 'bg', 4.5, 'Icona/etichetta correct su sfondo'),
    ('correct', 'surface', 4.5, 'Icona/etichetta correct su card'),
    ('correct', 'correctSoft', 4.5, 'Icona/etichetta correct su correctSoft'),
    ('ink', 'correctSoft', 7, 'Testo corpo su correctSoft'),
    ('incorrect', 'bg', 4.5, 'Icona/etichetta incorrect su sfondo'),
    ('incorrect', 'surface', 4.5, 'Icona/etichetta incorrect su card'),
    ('incorrect', 'incorrectSoft', 4.5, 'Icona/etichetta incorrect su incorrectSoft'),
    ('ink', 'incorrectSoft', 7, 'Testo corpo su incorrectSoft'),
    ('partial', 'bg', 4.5, 'Icona/etichetta partial su sfondo'),
    ('partial', 'surface', 4.5, 'Icona/etichetta partial su card'),
    ('partial', 'partialSoft', 4.5, 'Icona/etichetta partial su partialSoft'),
    ('ink', 'partialSoft', 7, 'Testo corpo su partialSoft'),
    ('info', 'bg', 4.5, 'Icona/etichetta info su sfondo'),
    ('info', 'surface', 4.5, 'Icona/etichetta info su card'),
    ('info', 'infoSoft', 4.5, 'Icona/etichetta info su infoSoft'),
    ('ink', 'infoSoft', 7, 'Testo corpo su infoSoft'),
    ('tip', 'bg', 4.5, 'Icona/etichetta tip su sfondo'),
    ('tip', 'surface', 4.5, 'Icona/etichetta tip su card'),
    ('tip', 'tipSoft', 4.5, 'Icona/etichetta tip su tipSoft'),
    ('ink', 'tipSoft', 7, 'Testo corpo su tipSoft'),
    ('warning', 'bg', 4.5, 'Icona/etichetta warning su sfondo'),
    ('warning', 'surface', 4.5, 'Icona/etichetta warning su card'),
    ('warning', 'warningSoft', 4.5, 'Icona/etichetta warning su warningSoft'),
    ('ink', 'warningSoft', 7, 'Testo corpo su warningSoft'),
    ('danger', 'bg', 4.5, 'Icona/etichetta danger su sfondo'),
    ('danger', 'surface', 4.5, 'Icona/etichetta danger su card'),
    ('danger', 'dangerSoft', 4.5, 'Icona/etichetta danger su dangerSoft'),
    ('ink', 'dangerSoft', 7, 'Testo corpo su dangerSoft'),
    ('toastInk', 'toastBg', 4.5, 'Testo del toast'),
    ('toastAccent', 'toastBg', 4.5, 'Azione del toast'),
    ('codeInk', 'codeBg', 7, 'Codice base'),
    ('codeInk', 'codeLineHighlight', 7, 'Codice su riga evidenziata'),
    ('codeKeyword', 'codeBg', 4.5, 'Sintassi keyword'),
    ('codeKeyword', 'codeLineHighlight', 4.5, 'Sintassi keyword su riga evidenziata'),
    ('codeString', 'codeBg', 4.5, 'Sintassi string'),
    ('codeString', 'codeLineHighlight', 4.5, 'Sintassi string su riga evidenziata'),
    ('codeComment', 'codeBg', 4.5, 'Sintassi comment'),
    ('codeComment', 'codeLineHighlight', 4.5, 'Sintassi comment su riga evidenziata'),
    ('codeNumber', 'codeBg', 4.5, 'Sintassi number'),
    ('codeNumber', 'codeLineHighlight', 4.5, 'Sintassi number su riga evidenziata'),
    ('codeFunction', 'codeBg', 4.5, 'Sintassi function'),
    ('codeFunction', 'codeLineHighlight', 4.5, 'Sintassi function su riga evidenziata'),
    ('codeType', 'codeBg', 4.5, 'Sintassi type'),
    ('codeType', 'codeLineHighlight', 4.5, 'Sintassi type su riga evidenziata'),
    ('codePunctuation', 'codeBg', 4.5, 'Sintassi punctuation'),
    ('codePunctuation', 'codeLineHighlight', 4.5, 'Sintassi punctuation su riga evidenziata'),
    ('onMark', 'mark', 7, 'Testo evidenziato'),
    ('focus', 'codeBg', 3, 'Focus su codice'),
    ('ink', 'highlightYellow', 4.5, 'Testo su evidenziazione giallo'),
    ('accent', 'highlightYellow', 4.5, 'Link dentro evidenziazione giallo'),
    ('knowledge', 'highlightYellow', 4.5, 'Citazione/termine dentro evidenziazione giallo'),
    ('highlightYellowLine', 'bg', 3, 'Linea di base evidenziazione giallo su bg (non testo)'),
    ('highlightYellowLine', 'surface', 3, 'Linea di base evidenziazione giallo su surface (non testo)'),
    ('highlightYellowLine', 'surfaceRaised', 3, 'Linea di base evidenziazione giallo su surfaceRaised (non testo)'),
    ('highlightYellowLine', 'infoSoft', 3, 'Linea di base evidenziazione giallo su infoSoft (non testo)'),
    ('highlightYellowLine', 'tipSoft', 3, 'Linea di base evidenziazione giallo su tipSoft (non testo)'),
    ('highlightYellowLine', 'warningSoft', 3, 'Linea di base evidenziazione giallo su warningSoft (non testo)'),
    ('ink', 'highlightYellow', 4.5, 'Icona di spunta sul campione giallo (tavolozza)'),
    ('ink', 'highlightGreen', 4.5, 'Testo su evidenziazione verde'),
    ('accent', 'highlightGreen', 4.5, 'Link dentro evidenziazione verde'),
    ('knowledge', 'highlightGreen', 4.5, 'Citazione/termine dentro evidenziazione verde'),
    ('highlightGreenLine', 'bg', 3, 'Linea di base evidenziazione verde su bg (non testo)'),
    ('highlightGreenLine', 'surface', 3, 'Linea di base evidenziazione verde su surface (non testo)'),
    ('highlightGreenLine', 'surfaceRaised', 3, 'Linea di base evidenziazione verde su surfaceRaised (non testo)'),
    ('highlightGreenLine', 'infoSoft', 3, 'Linea di base evidenziazione verde su infoSoft (non testo)'),
    ('highlightGreenLine', 'tipSoft', 3, 'Linea di base evidenziazione verde su tipSoft (non testo)'),
    ('highlightGreenLine', 'warningSoft', 3, 'Linea di base evidenziazione verde su warningSoft (non testo)'),
    ('ink', 'highlightGreen', 4.5, 'Icona di spunta sul campione verde (tavolozza)'),
    ('ink', 'highlightBlue', 4.5, 'Testo su evidenziazione blu'),
    ('accent', 'highlightBlue', 4.5, 'Link dentro evidenziazione blu'),
    ('knowledge', 'highlightBlue', 4.5, 'Citazione/termine dentro evidenziazione blu'),
    ('highlightBlueLine', 'bg', 3, 'Linea di base evidenziazione blu su bg (non testo)'),
    ('highlightBlueLine', 'surface', 3, 'Linea di base evidenziazione blu su surface (non testo)'),
    ('highlightBlueLine', 'surfaceRaised', 3, 'Linea di base evidenziazione blu su surfaceRaised (non testo)'),
    ('highlightBlueLine', 'infoSoft', 3, 'Linea di base evidenziazione blu su infoSoft (non testo)'),
    ('highlightBlueLine', 'tipSoft', 3, 'Linea di base evidenziazione blu su tipSoft (non testo)'),
    ('highlightBlueLine', 'warningSoft', 3, 'Linea di base evidenziazione blu su warningSoft (non testo)'),
    ('ink', 'highlightBlue', 4.5, 'Icona di spunta sul campione blu (tavolozza)'),
    ('ink', 'highlightPink', 4.5, 'Testo su evidenziazione rosa'),
    ('accent', 'highlightPink', 4.5, 'Link dentro evidenziazione rosa'),
    ('knowledge', 'highlightPink', 4.5, 'Citazione/termine dentro evidenziazione rosa'),
    ('highlightPinkLine', 'bg', 3, 'Linea di base evidenziazione rosa su bg (non testo)'),
    ('highlightPinkLine', 'surface', 3, 'Linea di base evidenziazione rosa su surface (non testo)'),
    ('highlightPinkLine', 'surfaceRaised', 3, 'Linea di base evidenziazione rosa su surfaceRaised (non testo)'),
    ('highlightPinkLine', 'infoSoft', 3, 'Linea di base evidenziazione rosa su infoSoft (non testo)'),
    ('highlightPinkLine', 'tipSoft', 3, 'Linea di base evidenziazione rosa su tipSoft (non testo)'),
    ('highlightPinkLine', 'warningSoft', 3, 'Linea di base evidenziazione rosa su warningSoft (non testo)'),
    ('ink', 'highlightPink', 4.5, 'Icona di spunta sul campione rosa (tavolozza)'),
]

# Scrittura a mano (components/handwriting.md): inchiostro >= 3:1 su ogni sfondo su cui si scrive (elemento grafico, WCAG 1.4.11)
_PEN = ('penInk', 'penBlue', 'penRed', 'penGreen')
_BGS = ('bg', 'surface', 'surfaceSunken', 'surfaceRaised', 'codeBg', 'infoSoft', 'tipSoft', 'warningSoft', 'knowledgeSoft',
        'correctSoft', 'incorrectSoft', 'partialSoft', 'dangerSoft', 'accentSoft',
        'highlightYellow', 'highlightGreen', 'highlightBlue', 'highlightPink')
PAIRS += [(p, b, 3, f'Inchiostro {p} su {b} (non testo)') for p in _PEN for b in _BGS]
PAIRS += [('penOn', p, 3, f'Spunta sul campione {p} (tavolozza)') for p in _PEN]

tok=json.load(open(pathlib.Path(__file__).resolve().parent.parent/"tokens.json"))["color"]
bad=0
for theme in ("light","dark"):
    c=tok[theme]
    for fg,bg,m,d in PAIRS:
        v=cr(c[fg],c[bg])
        if v<m:
            bad+=1; print(f"FAIL {theme}: {fg} {c[fg]} su {bg} {c[bg]} = {v:.2f} < {m} ({d})")
    print(f"{theme}: {len(PAIRS)} coppie controllate")
print("OK" if not bad else f"{bad} coppie sotto soglia"); sys.exit(1 if bad else 0)
