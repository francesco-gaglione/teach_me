import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.dependencies
import org.gradle.kotlin.dsl.configure
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

/** Pure JVM Kotlin module (no Android dependencies). */
class KotlinLibraryConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("org.jetbrains.kotlin.jvm")
            pluginManager.apply("org.jlleitschuh.gradle.ktlint")
            extensions.configure<KotlinJvmProjectExtension> {
                jvmToolchain(21)
            }
            dependencies {
                add("testImplementation", libs.lib("junit"))
            }
        }
    }
}
