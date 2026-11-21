import com.android.build.api.dsl.CommonExtension
import org.gradle.api.JavaVersion
import org.gradle.api.Project
import org.gradle.api.artifacts.VersionCatalog
import org.gradle.api.artifacts.VersionCatalogsExtension
import org.gradle.kotlin.dsl.getByType

internal val Project.libs: VersionCatalog
    get() = extensions.getByType<VersionCatalogsExtension>().named("libs")

internal fun VersionCatalog.lib(alias: String) = findLibrary(alias).get()

internal fun VersionCatalog.plugin(alias: String) = findPlugin(alias).get().get().pluginId

// ADR 0001 says compileSdk 35, but the current stable AndroidX/OkHttp releases require compileSdk 37.
// compileSdk only affects which APIs can be compiled against; runtime behavior is set by TARGET_SDK.
internal const val COMPILE_SDK = 37
internal const val TARGET_SDK = 35
internal const val MIN_SDK = 29

/** Settings shared by application and library modules. */
internal fun Project.configureAndroidCommon(extension: CommonExtension) {
    extension.apply {
        compileSdk = COMPILE_SDK
        defaultConfig.minSdk = MIN_SDK
        compileOptions.sourceCompatibility = JavaVersion.VERSION_21
        compileOptions.targetCompatibility = JavaVersion.VERSION_21
        testOptions.unitTests.isReturnDefaultValues = true
        lint.warningsAsErrors = true
        lint.abortOnError = true
        lint.checkDependencies = false
        // androidx.lifecycle ships a lint check built against an older Kotlin analysis API; lint skips it
        // and reports an error. Nothing we can fix on our side.
        lint.disable += "ObsoleteLintCustomCheck"
        // "A newer version is available" checks: with warningsAsErrors every upstream release would
        // break the build. Versions are pinned on purpose and upgraded as a decision.
        lint.disable += listOf("AndroidGradlePluginVersion", "GradleDependency", "NewerVersionAvailable")
    }
    pluginManager.apply("org.jlleitschuh.gradle.ktlint")
}
