# ADR 0013 — Lettura ad alta voce delle lezioni

- Stato: accettata
- Data: 2026-11-13
- Issue Gitea #8. Il proprietario ha deciso i punti 1-7 il 2026-11-13, dopo aver ascoltato i campioni; i punti 8-14 sono scelte del tech lead che ne derivano. Contratto: `docs/contracts/audio.md`.
- Aggiornata il 2026-11-16: il punto 1 passa al modello 1.7B (sezione «Aggiornamento 2026-11-16» in fondo).

## Problema

Una lezione si può solo leggere. Il proprietario vuole anche ascoltarla, con una voce naturale, che sembri una persona vera, generata in casa e non da un servizio esterno a pagamento.

## Prove fatte

Sul server di casa (Ryzen 7 5800H, 8 core / 16 thread, 15 GB, nessuna GPU utile) è stato provato **Qwen3-TTS 0.6B CustomVoice** (Apache-2.0) con il motore in C `gabriele-mastrapasqua/qwen3-tts` (MIT), in un container:

| Testo | Precisione | Audio | Generazione | RTF |
|---|---|---|---|---|
| 1131 caratteri | piena, 8 thread | 78,6 s | 194 s | 2,47 |
| 1131 caratteri | piena, 16 thread | 78,6 s | 215 s | 2,73 |
| 890 caratteri | mista int8/int4 | 64,2 s | 78 s | 1,22 |

Memoria di picco circa 1,5 GB. Con 8 thread va **più veloce** che con 16 (il collo di bottiglia è la memoria). Piper e Kokoro sono veloci ma più robotici; Chatterbox e XTTS-v2 sono troppo lenti senza GPU; gli altri modelli recenti richiedono una GPU.

## Scelta

1. **Motore: Qwen3-TTS 0.6B a precisione piena, sul server di casa.** Il proprietario ha preferito la qualità della precisione piena a quella quantizzata, più veloce. *Sostituito il 2026-11-16 dal modello 1.7B: vedi «Aggiornamento 2026-11-16».*
2. **Voce predefinita `vivian`, scelta dall'utente nelle Impostazioni.** La voce è **per utente** (salvata sul backend, vale su tutti i suoi dispositivi). Le Impostazioni offrono solo un **elenco curato** di voci che suonano bene in italiano, ciascuna con un campione da ascoltare.
3. **Si leggono** `paragraph`, `callout` (titolo e testo), `heading` e l'`alt` dei `diagram`. Tutto con la voce italiana, anche i termini inglesi.
4. **«Ascolta» su ogni blocco leggibile** e **«Ascolta la lezione»** con passaggio automatico al blocco successivo. La lettura continua parte dal blocco in cima allo schermo, scorre la pagina sul blocco letto e si ferma al primo esercizio obbligatorio non svolto.
5. **L'audio si genera in anticipo**: subito dopo la generazione di una lezione, in background. Le lezioni nate prima di questa funzione si preparano alla prima apertura. Un blocco non ancora pronto mostra «In preparazione».
6. **Cambio di voce:** le lezioni nuove usano subito la nuova voce; una lezione esistente si rigenera quando la si apre, e nel frattempo un blocco già pronto con un'altra voce si ascolta con quella. L'audio vecchio resta in cache: tornare alla voce precedente è immediato.
7. **Velocità** da 0,75× a 1,5× (tono invariato), salvata **sul dispositivo**. La lettura **si ferma** uscendo dalla lezione, avviando un altro blocco e mandando l'app in secondo piano: niente riproduzione a schermo spento.
8. **Motore sostituibile.** Nel backend la sintesi è una porta (`SpeechSynthesizer`) in `domain::ports`, come le altre porte (ADR 0002); Qwen3-TTS è un adapter HTTP verso un servizio a parte. Le voci sono **id opachi** che il backend dichiara (`GET /tts/voices`): né l'app né i contratti conoscono `vivian`. Cambiare modello significa scrivere un adapter e un nuovo elenco di voci; nient'altro cambia.
9. **Cache per contenuto.** Ogni clip è indirizzata da un hash di testo parlato, motore, modello, voce e parametri. Cambio di voce, cambio di modello e ripresa dopo un riavvio non richiedono logica speciale: manca la clip, si genera.
10. **Il testo parlato lo decide il backend**, una sola volta (`audio.md` §2): markdown inline ripulito, citazioni tolte, termini del glossario letti col loro testo.
11. **Coda audio interna, non un `Job`.** La generazione dell'audio non è un'operazione chiesta dall'utente e non deve far sembrare rotta una lezione: non compare in `GET /topics/{id}/jobs` e non usa la chiave LLM. Un solo lavoratore, una clip alla volta, in ordine di richiesta; la lezione aperta per ultima passa in testa. Le clip già fatte restano: dopo un riavvio si riparte dalla prima mancante. Un blocco che fallisce 3 volte resta `failed` finché la lezione non viene riaperta.
12. **Formato:** Ogg Opus mono a 32 kbit/s (circa 0,24 MB al minuto, contro 2,9 MB del WAV). Le clip stanno in un volume Docker proprio, **fuori dal backup** del database: sono una cache rigenerabile.
13. **Servizio `tts` nel compose**, non esposto all'esterno (lo raggiunge solo il backend), con commit del motore fissato, modello in un volume scaricato al primo avvio, 8 thread e priorità bassa per non pesare su backend e altri servizi di casa. Il motore deve accettare richieste lunghe: il limite predefinito di 60 s d'audio per richiesta taglierebbe in silenzio i paragrafi lunghi. Senza `TEACH_ME_TTS_URL` la funzione è spenta e l'app nasconde i comandi di ascolto.
14. **Seme fisso** per la sintesi, così due generazioni dello stesso testo con la stessa voce danno lo stesso audio.

## Alternative scartate

- **Sintesi di Android sul dispositivo**: niente server e niente attese, ma la voce suona sintetica.
- **Piper o Kokoro sul server**: abbastanza veloci per generare al volo, ma meno naturali.
- **Qwen3-TTS quantizzato** (circa due volte più veloce): il proprietario ha preferito la qualità piena.
- **Generazione al primo «Ascolta»**: niente CPU sprecata, ma ogni primo ascolto attenderebbe 1-3 minuti.
- **Audio in streaming durante la generazione**: con RTF intorno a 2,5 la riproduzione si interromperebbe di continuo.
- **Voce per dispositivo**: l'audio si genera sul server, quindi voci diverse per dispositivo moltiplicherebbero generazione e spazio.
- **Generazione come `Job` di `jobs.md`**: ne erediterebbe regole pensate per operazioni chieste dall'utente (retry manuale, fallimento visibile nella lista dei job del Topic).

## Conseguenze

- Circa 2,5 secondi di CPU per ogni secondo d'audio: una lezione di 10-15 minuti di ascolto richiede 25-40 minuti di lavoro in background su 8 thread.
- Un servizio in più sul server, circa 1,5 GB di RAM in uso e 2,4 GB di modello su disco.
- La cache delle clip si pulisce da sola (issue #20, 2026-11-15). «Uso» di una clip è l'ultima volta che è stata indicata a un utente in un manifest o all'apertura di una lezione, registrata con precisione di un giorno. Un passaggio giornaliero (il primo cinque minuti dopo l'avvio) cancella riga e file delle clip non usate da 90 giorni; i campioni delle voci in elenco restano sempre. Una clip cancellata che serve di nuovo si rigenera come una mai fatta (punto 9). Lo stesso passaggio cancella i file senza riga più vecchi di 24 ore (per esempio dopo il ripristino di un backup del database); i più giovani possono essere una clip in registrazione. Il controllo dell'ultimo uso è nello stesso statement che cancella la riga e un lock in memoria lo ordina rispetto ai manifest, così il passaggio non cancella mai una clip appena promessa. Con la lettura ad alta voce spenta (`TEACH_ME_TTS_URL` vuoto) il passaggio non gira. Le concessioni salvate prima di questa versione hanno la data della prima concessione, non dell'ultimo uso: nessuna aveva ancora 90 giorni, quindi non serve una migrazione.
- Il modello vocale è fissato a una revisione di Hugging Face (`85e237c…`, `deploy/tts/Dockerfile`, issue #20). Cambiarla è una decisione: le clip in cache sono indirizzate dal nome del modello, non dalla revisione, e resterebbero con la voce dei pesi vecchi.
- Nuovi contratti: `audio.md`; aggiunte additive a `users.md` (voce dell'utente) e `lessons.md` (l'apertura di una lezione prepara l'audio).
- Nuova dipendenza Android: Media3 ExoPlayer.

## Aggiornamento 2026-11-16 — modello 1.7B

Il proprietario ha chiesto se esistesse un'alternativa con un italiano molto migliore. Prove sul server di casa, stessi testi della tabella iniziale, voce `vivian`, seme 42, 8 thread, altri lavori TTS fermi:

| Motore | RTF (testo medio / lungo) | Note |
|---|---|---|
| Qwen3-TTS 0.6B, precisione piena (prima scelta) | 2,7 / 2,5 | |
| **Qwen3-TTS 1.7B CustomVoice, precisione piena** | 4,3 / 3,9 | parla più lento: per lo stesso testo circa 2 volte il tempo del 0.6B; picco di memoria 2,8 GB |
| Qwen3-TTS 1.7B `int8` | 2,3 / 2,2 | stesso tempo del 0.6B, ma quantizzato |
| OmniVoice (k2-fsa) via VoiceStudio | 3,4–14,9 | pesi CC BY-NC |
| Kyutai Pocket TTS `italian_24l` | 0,8–1,0 | una sola voce italiana (maschile); clonazione dietro termini Hugging Face |

Nessun modello aperto di altre famiglie risulta nettamente migliore sull'italiano e adatto alla CPU. Nel report tecnico di Qwen3-TTS il 1.7B CustomVoice dimezza circa le parole sbagliate in italiano (WER 2,55 → 1,36).

**Scelta del proprietario:** Qwen3-TTS **1.7B CustomVoice a precisione piena**, stesso motore C e stesso commit. I punti 2-14 restano validi. Il modello è fissato alla revisione `0c0e3051` (`deploy/tts/Dockerfile`), il servizio ha un limite di memoria di 5 GB e la chiave della cache usa il modello `1.7b-customvoice`.

**Conseguenze:** circa 4 secondi di CPU per ogni secondo d'audio, cioè circa 50-80 minuti di lavoro in background per una lezione di 10-15 minuti d'ascolto. Il primo avvio scarica circa 4,5 GB. Le clip del 0.6B non vengono usate come ripiego, perché il ripiego di `audio.md` §3 vale solo per un'altra voce: una lezione già ascoltata mostra «In preparazione» finché non viene rigenerata alla sua apertura. Il proprietario ha accettato questa pausa. Le clip vecchie scadono da sole dopo 90 giorni senza uso.
