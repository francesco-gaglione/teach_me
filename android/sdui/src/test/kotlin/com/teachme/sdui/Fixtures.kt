package com.teachme.sdui

import java.io.File

/** The contract fixtures (docs/contracts/fixtures): single source of truth, read straight from the repo. */
object Fixtures {
    val dir: File =
        generateSequence(File(System.getProperty("user.dir").orEmpty()).absoluteFile) { it.parentFile }
            .map { File(it, "docs/contracts/fixtures") }
            .first { it.isDirectory }

    /** Every lesson-*.json in the directory, discovered like the Rust side (sdui_fixtures.rs). */
    val validLessons: List<String> =
        dir
            .listFiles { f -> f.isFile && f.name.startsWith("lesson-") && f.name.endsWith(".json") }!!
            .map { it.name }
            .sorted()
            .also { check(it.isNotEmpty()) { "no lesson-*.json fixtures found in $dir" } }

    fun text(name: String): String = File(dir, name).readText()

    fun invalid(): List<File> =
        File(dir, "invalid")
            .listFiles { f ->
                f.name.endsWith(".json") &&
                    !f.name.endsWith(".expected.json")
            }!!
            .sortedBy { it.name }
}
