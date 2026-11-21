package com.teachme.core.model

import java.time.Instant

/** A folder of Topics (`docs/contracts/folders-topics.md` §1). */
data class Folder(
    val id: String,
    val name: String,
    val position: Int,
    val topicCount: Int,
    val createdAt: Instant,
    val updatedAt: Instant,
)
