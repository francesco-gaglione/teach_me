import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import java.util.Properties

plugins {
    id("teachme.android.application")
    id("teachme.android.compose")
    id("teachme.android.hilt")
    alias(libs.plugins.hilt)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.aboutlibraries)
}

android {
    namespace = "com.teachme.app"

    defaultConfig {
        applicationId = "com.teachme.app"
        versionCode = 1
        versionName = "0.1.0"
    }

    buildFeatures {
        buildConfig = true
    }

    testOptions.unitTests.isIncludeAndroidResources = true

    // Release signing reads android/keystore.properties (git-ignored). Without it the release APK stays unsigned.
    val keystoreProps =
        Properties().apply {
            val f = rootProject.file("keystore.properties")
            if (f.isFile) f.inputStream().use { load(it) }
        }
    val hasKeystore = keystoreProps.containsKey("storeFile")
    signingConfigs {
        if (hasKeystore) {
            create("release") {
                storeFile = rootProject.file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            if (hasKeystore) {
                signingConfig = signingConfigs.getByName("release")
            } else {
                logger.warn("WARNING: android/keystore.properties not found: the release APK will be UNSIGNED.")
            }
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
}

/**
 * Copies the SDUI contract fixtures (docs/contracts/fixtures, top-level lessons only) into the assets of
 * the DEBUG variant, so the app can open them without a backend. They never reach a release build.
 */
abstract class CopyLessonFixtures : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val fixtures: DirectoryProperty

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun copy() {
        val target =
            output.get().asFile.resolve("fixtures").apply {
                deleteRecursively()
                mkdirs()
            }
        fixtures.get().asFile.listFiles { f -> f.isFile && f.extension == "json" }.orEmpty().forEach {
            it.copyTo(target.resolve(it.name))
        }
    }
}

/**
 * Puts the generated `licenses.json` (AboutLibraries export of the RELEASE runtime classpath, plus the
 * hand-declared fonts/icons in licenses-config) in the assets of every variant. The file is never in git.
 */
abstract class CopyLicenses : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val licenses: RegularFileProperty

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun copy() {
        val dir =
            output.get().asFile.apply {
                deleteRecursively()
                mkdirs()
            }
        val src = licenses.get().asFile
        // Build-time guard: an empty export would silently ship a screen that says "not available".
        check(src.isFile && Regex("\"uniqueId\"").findAll(src.readText()).count() >= 10) {
            "Generated licenses.json is missing or has too few libraries: $src"
        }
        src.copyTo(dir.resolve("licenses.json"))
    }
}

androidComponents {
    onVariants { variant ->
        val copy =
            tasks.register<CopyLicenses>("copy${variant.name.replaceFirstChar { it.uppercase() }}Licenses") {
                licenses.set(layout.buildDirectory.file("generated/licenses/licenses.json"))
                output.set(layout.buildDirectory.dir("generated/licenseAssets/${variant.name}"))
            }
        copy.configure { dependsOn(tasks.named("exportLibraryDefinitionsRelease")) }
        variant.sources.assets?.addGeneratedSourceDirectory(copy, CopyLicenses::output)
    }
    onVariants(selector().withBuildType("debug")) { variant ->
        val copy =
            tasks.register<CopyLessonFixtures>(
                "copy${variant.name.replaceFirstChar { it.uppercase() }}LessonFixtures",
            ) {
                fixtures.set(rootProject.layout.projectDirectory.dir("../docs/contracts/fixtures"))
                output.set(layout.buildDirectory.dir("generated/lessonFixtures/${variant.name}"))
            }
        variant.sources.assets?.addGeneratedSourceDirectory(copy, CopyLessonFixtures::output)
    }
}

dependencies {
    implementation(project(":core:designsystem"))
    implementation(project(":core:model"))
    implementation(project(":core:network"))
    implementation(project(":core:data"))
    implementation(project(":core:ui"))
    implementation(project(":feature:login"))
    implementation(project(":feature:settings"))
    implementation(project(":feature:home"))
    implementation(project(":feature:folder"))
    implementation(project(":feature:chat"))
    implementation(project(":feature:mission"))
    implementation(project(":feature:lesson"))
    implementation(project(":feature:review"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.hilt.navigation.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.androidx.work.runtime)
    implementation(libs.androidx.hilt.work)
    ksp(libs.androidx.hilt.compiler)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.robolectric)
}

// Robolectric instruments the JDK classes with ASM: it needs the JDK the project compiles for (21), not a newer one.
tasks.withType<Test>().configureEach {
    javaLauncher.set(javaToolchains.launcherFor { languageVersion.set(JavaLanguageVersion.of(21)) })
}

aboutLibraries {
    collect {
        // Non-Maven assets (fonts, icons) are declared by hand here.
        configPath.set(layout.projectDirectory.dir("licenses-config"))
    }
    export {
        outputFile.set(layout.buildDirectory.file("generated/licenses/licenses.json"))
    }
}
