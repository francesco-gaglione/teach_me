# `code`

## Scopo didattico
Mostrare un **esempio minimo** concreto prima della regola (dal concreto all'astratto) e dare un oggetto su cui fare previsioni (le domande dei quiz rimandano a righe). Contiguità spaziale (Mayer): il codice sta accanto al testo che lo spiega; il numero di riga permette di riferirsi a un punto preciso senza ambiguità.

## Anatomia

```
┌──────────────────────────────────────────────┐ fill codeBg, raggio sm (6), nessun bordo
│ rust                                   [ ⎘ ]  │ intestazione 48 dp: lingua (ui.labelS, codeComment) + IconButton "Copia codice"
├──────────────────────────────────────────────┤
│ 1 │ let a = String::from("ciao");             │ code.block (JetBrains Mono 400 14/22), codeInk
│ 2 │ let b = a;      ← riga evidenziata   ▒▒▒▒ │ gutter numeri: codeComment, 32 dp, non selezionabile
│ 3 │ println!("{}", a);                        │ corpo: padding 12 (v) × 16 (h) ; scroll orizzontale
└──────────────────────────────────────────────┘ ▒ = sfumatura 24 dp a destra se c'è overflow
Caption (md): "Il compilatore rifiuta la riga 3."   ui.body, inkSoft, 8 dp sotto
```

| Misura / token | Valore |
|---|---|
| Fill | `codeBg`; contrasto `codeInk`/`codeBg` 13,3 (light) / 13,1 (dark) |
| Raggio | `sm` (6 dp); nessun bordo; nessuna ombra |
| Intestazione | 48 dp; lingua a sinistra (padding 16), `Copia` IconButton 48 × 48 (icona `copy` 20 dp `inkSoft`) a destra; separatore `codeLine` 1 dp. **Senza** `language` o `text`: nessuna etichetta lingua, intestazione presente solo per il bottone |
| Numeri di riga | solo se il codice ha **≥ 3 righe**; gutter 32 dp allineato a destra, 12 dp di spazio dal testo; `codeComment` (5,4 / 6,8); `userSelectable` no (copiare non include numeri) |
| Evidenziatore di riga | **non nel contratto v1**; se aggiunto (proposta `highlight_lines`) fill `codeLineHighlight` sulla riga intera |
| Scorrimento | orizzontale, senza a capo; scrollbar nascosta; sfumatura destra 24 dp (da `codeBg` a trasparente) solo se c'è overflow |
| Righe visibili | fino a `codeCollapsedLines` 16; oltre: collassato con sfumatura in basso 32 dp e Button testuale «Mostra le altre N righe» (48 dp) sotto il blocco; espanso: «Mostra meno». Nessuno scorrimento verticale interno (scroll annidato = trappola) |
| Margine | 24 dp sopra/sotto (`blockGap`); caption a 8 dp |

### Evidenziazione sintattica
Colori `codeKeyword`, `codeString`, `codeComment`, `codeNumber`, `codeFunction`, `codeType`, `codePunctuation` (tutti ≥ 4,5:1 su `codeBg`, tabella in design-system §2.4). Un tokenizer leggero per `language` ∈ {`rust`, `kotlin`, `java`, `python`, `javascript`, `typescript`, `c`, `cpp`, `go`, `sql`, `json`, `bash`, `toml`, `yaml`}; qualunque altro valore (incl. `text`, `output`) → solo `codeInk`. Il colore è una **ridondanza** (mai l'unico veicolo di significato: se una parola importante è colorata, la spiegazione la nomina). Nessun bold/italic nel codice, nessuna ligatura (`fontFeatureSettings = "calt 0, liga 0"`), tabulazioni = 4 spazi.

## Stati

| Stato | Aspetto |
|---|---|
| Normale | come sopra |
| Premuto "Copia" | fill `surfacePressed` sul bottone; icona → `check` 1,5 s con `contentDescription` «Copiato» (live region polite); su Android < 13 anche toast «Codice copiato» |
| Focus | il contenitore scorrevole è **focusable** (tastiera): anello `focus` 3 dp attorno al corpo; ←/→ scorrono di 48 dp, Home/End agli estremi, ↑↓ lasciano il focus |
| Collassato / espanso | vedi sopra |
| Errore di tokenizer | testo senza colori (mai crash) |

## Interazioni
Tocco su Copia; selezione lunga del testo (senza numeri); scorrimento orizzontale a trascinamento; bottone espandi.

## Accessibilità
- Il blocco è **un solo nodo** di lettura: `contentDescription` iniziale «Codice <lingua>, N righe», `text` = codice integrale (non una riga per volta); numeri di riga esclusi. Azione custom «Copia codice» e (se c'è overflow) «Scorri a destra/sinistra».
- Il bottone Copia è un nodo separato, dopo il codice nell'ordine di lettura.
- `lang`: il codice non è mai marcato con la lingua naturale (è codice); la `caption` sì, se c'è `lang`.
- Font 200 %: righe a 28 sp di interlinea; scorrimento orizzontale più frequente (nessun a capo forzato: perderebbe l'indentazione).
- Contrasto: vedi design-system §2.4; il focus su `codeBg` 6,6 / 9,0.

## Contenuti al limite
- 3000 caratteri: se > 16 righe collassato; il blocco non supera mai ~16 × 22 + 96 = 450 dp da collassato.
- Riga di 100 caratteri: a 14 sp (≈ 8,4 dp/car.) = 840 dp: scorre. Su tablet con colonna 600 dp ci stanno ≈ 64 caratteri senza scorrere (compact ≈ 35).
- Codice vuoto/solo spazi: non si disegna.
- Tabulazioni miste, `\r\n`: normalizzati.

## Linee guida per il Generator
- **Minimo**: 3-12 righe, **≤ 60 caratteri per riga** (limite del contratto 100). Un solo concetto; niente boilerplate che non sia l'oggetto.
- `language` sempre indicato; `text` per output di programma, comandi con output, diagrammi ASCII (≤ 60 colonne). Per il comando di shell, `bash`.
- Il paragrafo che precede introduce *cosa osservare*; il paragrafo (o quiz) successivo usa le righe ("alla riga 2"): i numeri compaiono con ≥ 3 righe, quindi se si rimanda a una riga l'esempio ha ≥ 3 righe.
- `caption`: una frase che dice cosa mostra («Il compilatore rifiuta la riga 3.»); non ripetere il codice a parole.
- Niente codice che richiede di scorrere in orizzontale; spezzare le righe lunghe come farebbe un formattatore.
- Codice sbagliato intenzionalmente: dirlo nel `caption` o nel paragrafo che precede, non lasciarlo intuire.
- Mai usare `code` per testo normale citato, dati tabulari o elenchi.
