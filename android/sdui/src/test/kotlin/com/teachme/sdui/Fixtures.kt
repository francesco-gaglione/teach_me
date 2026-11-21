package com.teachme.sdui

import java.io.File

/** The contract fixtures (docs/contracts/fixtures): single source of truth, read straight from the repo. */
object Fixtures {
    val dir: File =
        generateSequence(File(System.getProperty("user.dir").orEmpty()).absoluteFile) { it.parentFile }
            .map { File(it, "docs/contracts/fixtures") }
            .first { it.isDirectory }

    val validLessons =
        listOf(
            "lesson-rust-ownership.json",
            "lesson-photo-exposure.json",
            "lesson-git-staging.json",
            "lesson-scientific-method.json",
        )

    fun text(name: String): String = File(dir, name).readText()

    fun invalid(): List<File> =
        File(dir, "invalid")
            .listFiles { f ->
                f.name.endsWith(".json") &&
                    !f.name.endsWith(".expected.json")
            }!!
            .sortedBy { it.name }
}
