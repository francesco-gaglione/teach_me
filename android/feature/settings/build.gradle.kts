plugins {
    id("teachme.android.feature")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "com.teachme.feature.settings"

    testOptions.unitTests.isIncludeAndroidResources = true
}

dependencies {
    implementation(project(":core:network"))
    implementation(project(":sdui"))
    implementation(libs.kotlinx.serialization.json)

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
