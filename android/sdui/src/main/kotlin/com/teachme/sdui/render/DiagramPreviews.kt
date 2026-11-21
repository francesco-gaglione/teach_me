package com.teachme.sdui.render

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.DiagramEdge
import com.teachme.sdui.model.DiagramNode

// The three figures of docs/contracts/fixtures/lesson-scientific-method.json, at a 600 dp reading column
// (tablet, 632 dp wide preview with the 16 dp margins of the frame) and at 360 dp (phone), light, dark, font 2.0.

private val FIG_CYCLE =
    Diagram(
        id = "fig_cycle",
        layout = "cycle",
        title = "Il ciclo del metodo scientifico",
        nodes =
            listOf(
                DiagramNode("osserva", "Osservazione", "qualcosa che non torna"),
                DiagramNode("ipotesi", "Ipotesi", "una risposta che si può smentire"),
                DiagramNode("esperimento", "Esperimento"),
                DiagramNode("analisi", "Analisi dei dati"),
            ),
        caption = "Dopo l'analisi non c'è un traguardo: i dati portano a una **nuova osservazione**.",
        alt = "Quattro tappe disposte in cerchio, collegate da frecce nello stesso verso.",
    )

private val FIG_TREE =
    Diagram(
        id = "fig_tree",
        layout = "tree",
        title = "Le variabili di un esperimento",
        nodes =
            listOf(
                DiagramNode("variabili", "Variabili"),
                DiagramNode("indipendente", "Indipendente", "la cambi tu: ore di luce"),
                DiagramNode("dipendente", "Dipendente", "la misuri: altezza della pianta"),
                DiagramNode("controllo", "Di controllo", "restano uguali: acqua, vaso"),
            ),
        edges = listOf("indipendente", "dipendente", "controllo").map { DiagramEdge("variabili", it) },
        caption = "Nell'esempio della pianta cambiano solo le ore di luce; acqua e vaso sono identici.",
        alt = "Un albero con la radice «Variabili» e tre rami: indipendente, dipendente, di controllo.",
    )

private val FIG_FLOW =
    Diagram(
        id = "fig_flow",
        layout = "flow",
        nodes =
            listOf(
                DiagramNode("dati", "Dati raccolti"),
                DiagramNode("confronto", "Confronto con l'ipotesi"),
                DiagramNode("regge", "L'ipotesi regge", "si ripete e si estende"),
                DiagramNode("smentita", "L'ipotesi è smentita", "se ne formula un'altra"),
            ),
        edges =
            listOf(
                DiagramEdge("dati", "confronto"),
                DiagramEdge("confronto", "regge", "d'accordo"),
                DiagramEdge("confronto", "smentita", "in disaccordo"),
            ),
        caption = "Un'ipotesi smentita non è un fallimento: restringe il campo delle risposte possibili.",
        alt = "Un percorso da sinistra a destra: dai dati al confronto con l'ipotesi, che si divide in due esiti.",
    )

/** A flow with an arrow that skips a level (it runs on a lane outside the nodes) and a three-node cycle. */
private val FIG_FLOW_SKIP =
    Diagram(
        id = "fig_skip",
        layout = "flow",
        title = "Dal codice al rilascio",
        nodes = listOf("Scrivi", "Testa", "Rilascia").map { DiagramNode(it.lowercase(), it) },
        edges =
            listOf(
                DiagramEdge("scrivi", "testa", "git push"),
                DiagramEdge("testa", "rilascia"),
                DiagramEdge("scrivi", "rilascia", "hotfix"),
            ),
        caption = "Il hotfix salta i test.",
        alt = "Tre tappe in fila: scrivi, testa, rilascia; un arco diretto da scrivi a rilascia salta i test.",
    )

private val FIG_CYCLE3 =
    FIG_CYCLE.copy(id = "fig_cycle3", title = null, nodes = FIG_CYCLE.nodes.take(3), caption = "Tre tappe.")

@Composable
private fun DiagramSamples(dark: Boolean) =
    PreviewFrame(dark) {
        DiagramBlockView(FIG_CYCLE)
        DiagramBlockView(FIG_TREE)
        DiagramBlockView(FIG_FLOW)
        DiagramBlockView(FIG_FLOW_SKIP)
        DiagramBlockView(FIG_CYCLE3)
    }

@Preview(name = "Diagram tablet light", widthDp = 632, heightDp = 1700, showBackground = true)
@Composable
private fun DiagramTabletLight() = DiagramSamples(false)

@Preview(name = "Diagram tablet dark", widthDp = 632, heightDp = 1700, showBackground = true)
@Composable
private fun DiagramTabletDark() = DiagramSamples(true)

@Preview(name = "Diagram phone light", widthDp = 392, heightDp = 2300, showBackground = true)
@Composable
private fun DiagramPhoneLight() = DiagramSamples(false)

@Preview(name = "Diagram phone dark", widthDp = 392, heightDp = 2300, showBackground = true)
@Composable
private fun DiagramPhoneDark() = DiagramSamples(true)

@Preview(name = "Diagram phone font 2.0", widthDp = 392, heightDp = 4200, showBackground = true, fontScale = 2f)
@Composable
private fun DiagramPhoneLarge() = DiagramSamples(false)

@Preview(name = "Diagram tablet font 2.0 dark", widthDp = 632, heightDp = 3400, showBackground = true, fontScale = 2f)
@Composable
private fun DiagramTabletLargeDark() = DiagramSamples(true)
