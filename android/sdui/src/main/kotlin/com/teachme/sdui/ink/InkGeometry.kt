package com.teachme.sdui.ink

import kotlin.math.roundToInt

/** A point in pixels, in whatever space the caller works (window or content: it must be the same everywhere). */
data class InkPoint(
    val x: Float,
    val y: Float,
)

/** A contract point: ten-thousandths of the block width. */
data class ContractPoint(
    val x: Int,
    val y: Int,
)

/** The current geometry of a top-level block: left edge, top edge and width of its column, in pixels. */
data class BlockGeometry(
    val left: Float,
    val top: Float,
    val width: Float,
) {
    /**
     * The contract coordinates of a screen point (handwriting.md section 5.2): both divided by the block width, rounded
     * to integers. Null when the width is not positive or the result is out of the allowed ranges.
     */
    fun toContract(
        x: Float,
        y: Float,
    ): ContractPoint? {
        if (width <= 0f) return null
        val cx = ((x - left) / width * InkLimits.SCALE).roundToInt()
        val cy = ((y - top) / width * InkLimits.SCALE).roundToInt()
        val point = ContractPoint(cx, cy)
        return point.takeIf { it.isAllowed() }
    }

    /** The screen point of contract coordinates, with the block's geometry as it is now. */
    fun toScreen(
        x: Int,
        y: Int,
    ): InkPoint = InkPoint(left + x.toFloat() / InkLimits.SCALE * width, top + y.toFloat() / InkLimits.SCALE * width)
}

fun ContractPoint.isAllowed(): Boolean = x in InkLimits.X_MIN..InkLimits.X_MAX && y in InkLimits.Y_MIN..InkLimits.Y_MAX

/** The vertical extent of a visible top-level block, in the same space as the pointer's y. */
data class BlockExtent(
    val id: String,
    val top: Float,
    val height: Float,
) {
    val bottom: Float get() = top + height
}

/**
 * Picks the block of a stroke from its first point (handwriting.md section 5.2). Each block has a full-width band that
 * runs from the midpoint of the gap above it to the midpoint of the gap below it (the first starts and the last ends
 * at infinity: the caller clips the writing area). A point in a gap therefore goes to the nearest block, and exactly
 * halfway to the one above.
 *
 * [blocks] are the visible blocks in document order. Returns null for an empty list.
 */
fun pickBlock(
    blocks: List<BlockExtent>,
    y: Float,
): String? {
    if (blocks.isEmpty()) return null
    for (i in 0 until blocks.lastIndex) {
        val bandBottom = (blocks[i].bottom + blocks[i + 1].top) / 2f
        if (y <= bandBottom) return blocks[i].id
    }
    return blocks.last().id
}
