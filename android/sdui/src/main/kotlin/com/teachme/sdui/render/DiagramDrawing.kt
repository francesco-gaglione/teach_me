package com.teachme.sdui.render

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.diagram.DRect
import com.teachme.sdui.diagram.DiagramPlan
import com.teachme.sdui.diagram.DiagramSpec
import com.teachme.sdui.diagram.PlacedEdge
import com.teachme.sdui.model.Diagram
import kotlin.math.sqrt

/** Draws a [DiagramPlan]: arrows on a canvas, nodes, edge labels, order numbers and the return icon on top. */
@Composable
internal fun DiagramDrawing(
    diagram: Diagram,
    plan: DiagramPlan,
) {
    val colors = TeachTheme.colors
    val strokeWidth = TeachTheme.stroke.control
    Box(Modifier.fillMaxWidth()) {
        Box(Modifier.align(Alignment.TopCenter).size(plan.width.dp, plan.height.dp)) {
            Canvas(Modifier.matchParentSize()) {
                plan.edges.forEach {
                    drawEdge(
                        it,
                        colors.inkSoft,
                        strokeWidth.toPx(),
                        DiagramSpec.ARROW_HEAD.dp.toPx(),
                    )
                }
            }
            plan.nodes.forEach { DiagramNodeView(diagram, it.index, it.rect) }
            plan.labels.forEach { label ->
                Box(
                    Modifier
                        .placed(label.rect)
                        .background(colors.surfaceSunken)
                        .padding(
                            horizontal = DiagramSpec.LABEL_PADDING_X.dp,
                            vertical = DiagramSpec.LABEL_PADDING_Y.dp,
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    TeachText(
                        diagram.edges[label.edge].label.orEmpty(),
                        style =
                            TeachTheme.typography.ui.bodyS
                                .forLang(diagram.lang),
                        color = colors.inkSoft,
                        textAlign = TextAlign.Center,
                    )
                }
            }
            // The circle grows with the font scale so the number is never cut (the text is not scaled down).
            val badge =
                with(
                    LocalDensity.current,
                ) {
                    maxOf(
                        DiagramSpec.BADGE.dp,
                        TeachTheme.typography.ui.labelS.lineHeight
                            .toDp(),
                    )
                }
            plan.nodes.forEach { node ->
                val order = node.order ?: return@forEach
                Box(
                    Modifier
                        .offset((node.rect.x - BADGE_OVERHANG).dp, (node.rect.y - BADGE_OVERHANG).dp)
                        .size(badge)
                        .background(colors.accent, TeachTheme.shapes.full),
                    contentAlignment = Alignment.Center,
                ) {
                    TeachText(order.toString(), style = TeachTheme.typography.ui.labelS, color = colors.onAccent)
                }
            }
            plan.returnIcon?.let { rect ->
                Box(Modifier.placed(rect).background(colors.surfaceSunken), contentAlignment = Alignment.Center) {
                    TeachIcon(
                        TeachIcons.Repeat,
                        contentDescription = null,
                        tint = colors.inkSoft,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                }
            }
        }
    }
}

/** How far an order number sticks out of the corner of its node (it stays inside the 16 dp frame padding). */
private const val BADGE_OVERHANG = 8f

private fun Modifier.placed(rect: DRect): Modifier = offset(rect.x.dp, rect.y.dp).size(rect.w.dp, rect.h.dp)

/** Every node looks the same: no special node, no semantic color (diagram.md "Nodo"). */
@Composable
private fun DiagramNodeView(
    diagram: Diagram,
    index: Int,
    rect: DRect,
) {
    val node = diagram.nodes[index]
    val colors = TeachTheme.colors
    Box(
        Modifier
            .placed(rect)
            .background(colors.surface, TeachTheme.shapes.md)
            .border(TeachTheme.stroke.control, colors.border, TeachTheme.shapes.md)
            .padding(horizontal = DiagramSpec.NODE_PADDING_X.dp, vertical = DiagramSpec.NODE_PADDING_Y.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            TeachText(
                node.label,
                modifier = Modifier.fillMaxWidth(),
                style =
                    TeachTheme.typography.ui.label
                        .forLang(diagram.lang),
                color = colors.ink,
                textAlign = TextAlign.Center,
            )
            node.detail?.takeIf { it.isNotBlank() }?.let {
                Spacer(Modifier.height(DiagramSpec.DETAIL_GAP.dp))
                TeachText(
                    it,
                    modifier = Modifier.fillMaxWidth(),
                    style =
                        TeachTheme.typography.ui.bodyS
                            .forLang(diagram.lang),
                    color = colors.inkSoft,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

private fun DrawScope.drawEdge(
    edge: PlacedEdge,
    color: androidx.compose.ui.graphics.Color,
    strokeWidth: Float,
    head: Float,
) {
    val points = edge.points.map { Offset(it.x.dp.toPx(), it.y.dp.toPx()) }.toMutableList()
    if (points.size < 2) return
    val tip = points.last()
    val before = points[points.size - 2]
    val dx = tip.x - before.x
    val dy = tip.y - before.y
    val length = sqrt(dx * dx + dy * dy)
    if (edge.arrow && length > 0f) {
        val ux = dx / length
        val uy = dy / length
        // The line stops at the base of the head so its rounded cap does not poke out of the tip.
        val base = Offset(tip.x - ux * head, tip.y - uy * head)
        points[points.lastIndex] = base
        val half = head / 2
        val triangle =
            Path().apply {
                moveTo(tip.x, tip.y)
                lineTo(base.x - uy * half, base.y + ux * half)
                lineTo(base.x + uy * half, base.y - ux * half)
                close()
            }
        drawPath(triangle, color)
    }
    val path =
        Path().apply {
            moveTo(points.first().x, points.first().y)
            points.drop(1).forEach { lineTo(it.x, it.y) }
        }
    drawPath(path, color, style = Stroke(width = strokeWidth, cap = StrokeCap.Round, join = StrokeJoin.Round))
}
