package com.teachme.core.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember

/** One [BringIntoViewRequester] per row id, so a list can scroll a freshly created row into view. */
@OptIn(ExperimentalFoundationApi::class)
class RowRequesters {
    private val requesters = mutableMapOf<String, BringIntoViewRequester>()

    fun of(id: String): BringIntoViewRequester = requesters.getOrPut(id) { BringIntoViewRequester() }
}

@Composable
fun rememberRowRequesters(): RowRequesters = remember { RowRequesters() }
