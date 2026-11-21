package com.teachme.sdui.logic

/**
 * Deterministic shuffle of `order_steps` (docs/design/components/order_steps.md, "Mescolamento
 * deterministico"): the same block id gives the same order on every device and every opening.
 */
object OrderShuffle {
    private const val FNV_OFFSET = 0x811C9DC5.toInt()
    private const val FNV_PRIME = 0x01000193
    private const val ZERO_SEED = 0x9E3779B9.toInt()

    /** FNV-1a, 32 bit, over the UTF-8 bytes. */
    fun fnv1a(text: String): Int {
        var hash = FNV_OFFSET
        for (b in text.toByteArray(Charsets.UTF_8)) {
            hash = (hash xor (b.toInt() and 0xFF)) * FNV_PRIME
        }
        return hash
    }

    /** Shuffles [correctOrder] for [blockId]; the result is never the solution and has at most n/2 steps in place. */
    fun shuffle(
        blockId: String,
        correctOrder: List<String>,
    ): List<String> {
        val n = correctOrder.size
        if (n < 2) return correctOrder
        var x = fnv1a(blockId).let { if (it == 0) ZERO_SEED else it }

        fun next(): Int {
            x = x xor (x shl 13)
            x = x xor (x ushr 17)
            x = x xor (x shl 5)
            return x
        }
        var p = correctOrder.toMutableList()
        for (i in n - 1 downTo 1) {
            val j = Integer.remainderUnsigned(next(), i + 1)
            val t = p[i]
            p[i] = p[j]
            p[j] = t
        }

        fun inPlace(list: List<String>) = list.indices.count { list[it] == correctOrder[it] }

        fun rotated(list: List<String>) = list.drop(1) + list.first()
        if (p == correctOrder) p = rotated(p).toMutableList()
        var attempts = 0
        while (inPlace(p) * 2 > n && attempts < n) {
            p = rotated(p).toMutableList()
            attempts++
        }
        // Rotating can come full circle to the solution; one more step always leaves it.
        if (p == correctOrder) p = rotated(p).toMutableList()
        return p
    }
}
