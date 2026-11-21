plugins {
    id("teachme.android.library")
    id("teachme.android.compose")
}

android {
    namespace = "com.teachme.core.designsystem"

    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
}

// No Material: the design system is built on androidx.compose.foundation (ADR 0003 / design-system.md).

dependencies {
    testImplementation(libs.kotlinx.serialization.json)

    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    // Newer Espresso: older ones call InputManager.getInstance(), removed on recent Android versions.
    androidTestImplementation(libs.androidx.test.espresso.core)
    debugImplementation(libs.compose.ui.test.manifest)
}
