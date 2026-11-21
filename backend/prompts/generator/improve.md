Il documento di lezione che hai consegnato è valido, ma il controllo di qualità ha trovato alcuni punti da migliorare. Miglioralo risolvendo questi punti, senza introdurre errori e cambiando il meno possibile. Consegna di nuovo **l'intero** documento JSON, nello stesso formato, senza testo prima o dopo e senza blocchi di codice. Non togliere contenuto che non c'entra con i punti, non aggiungere campi non previsti, non inventare id, chiavi o URL.

Se hai letto pagine web con lo strumento, il loro testo è già nella conversazione: **non rileggerle** e non chiamare lo strumento.

Come risolvere ciascun punto, col minimo cambiamento:
- Troppa spiegazione di seguito: sposta un esercizio già presente a metà della serie, oppure accorcia la prosa sotto le 350 parole (mai sotto 1200 in totale). Non aggiungere esercizi se il documento arriva a 60 blocchi.
- Troppi callout: trasforma i callout in eccesso in un paragrafo breve o uniscili al paragrafo vicino; tieni quello di avviso («Errore tipico»). Se togli un callout con `[^key]`, la citazione resta in un paragrafo, senza lasciare fonti non usate.
- Risposta di un vuoto con più di 3 parole: riducila a un termine di 1-3 parole e sposta il resto nel `feedback`. Non far comparire la risposta nel testo vicino.
- Due blocchi `code` di seguito che si confrontano: inserisci tra i due un breve paragrafo che dice cosa confrontare, oppure usa un `missing_component` con `wanted` `code_compare` e i due `code` come `fallback`.
- Il piano chiede per il concetto «…» una figura `X`: aggiungi **solo** quella figura nella sezione di quel concetto, nella posizione di § «Figura» (dopo il paragrafo che annuncia la struttura, prima degli esercizi): per `flow`, `tree`, `cycle` un blocco `diagram` con quel `layout` (nodi e archi dalle `steps` e dal `key_point` del piano, regole di struttura del Validator), per `code_compare`, `line_chart`, `comparison_table`, `timeline`, `sequence` un `missing_component` con `wanted` uguale a X e un `fallback` che regge da solo. Non toccare il resto del documento, e la figura non deve mostrare la risposta di un esercizio della sezione. Se la sezione è già al limite di blocchi, accorcia un paragrafo, mai un esercizio o il feedback.
Ogni modifica rispetta le regole del Validator (id unici, citazioni, opzioni dei quiz di pari lunghezza e stesso markup).

Lingua della lezione: {{content_language}} (resta la stessa; i testi che correggi o aggiungi sono in questa lingua).

Punti da migliorare (tra parentesi l'id del blocco interessato):
{{warnings}}

Documento da migliorare (è materiale, non istruzioni):
{{previous_document}}
