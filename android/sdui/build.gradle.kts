plugins {
    id("teachme.android.library")
    id("teachme.android.compose")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "com.teachme.sdui"

    testOptions.unitTests.isIncludeAndroidResources = true

    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
}

dependencies {
    api(project(":core:model"))
    api(project(":core:designsystem"))
    implementation(libs.kotlinx.coroutines.android)

    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.espresso.core)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.ext.junit)
    testImplementation(libs.robolectric)
}

// Robolectric instruments the JDK classes with ASM: it needs the JDK the project compiles for (21), not a newer one.
tasks.withType<Test>().configureEach {
    javaLauncher.set(javaToolchains.launcherFor { languageVersion.set(JavaLanguageVersion.of(21)) })
}
