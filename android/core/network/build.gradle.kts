plugins {
    id("teachme.android.library")
    id("teachme.android.hilt")
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "com.teachme.core.network"
}

dependencies {
    api(project(":core:model"))
    api(libs.ktor.client.core)
    implementation(libs.ktor.client.okhttp)
    implementation(libs.ktor.client.content.negotiation)
    implementation(libs.ktor.serialization.kotlinx.json)
    implementation(libs.kotlinx.coroutines.android)

    testImplementation(libs.ktor.client.mock)
    testImplementation(libs.kotlinx.coroutines.test)
}
