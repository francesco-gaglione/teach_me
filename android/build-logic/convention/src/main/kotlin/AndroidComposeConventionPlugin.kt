import com.android.build.api.dsl.CommonExtension
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure
import org.gradle.kotlin.dsl.dependencies

/** Enables Compose in an Android module (apply after the application/library convention). */
class AndroidComposeConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply(libs.plugin("kotlin-compose"))
            extensions.configure<CommonExtension> {
                buildFeatures.compose = true
            }
            dependencies {
                val bom = platform(libs.lib("compose-bom"))
                add("implementation", bom)
                add("androidTestImplementation", bom)
                add("implementation", libs.lib("compose-ui"))
                add("implementation", libs.lib("compose-foundation"))
                add("implementation", libs.lib("compose-ui-tooling-preview"))
                add("debugImplementation", libs.lib("compose-ui-tooling"))
            }
        }
    }
}
