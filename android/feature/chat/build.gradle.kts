plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.chat"
}

dependencies {
    implementation(project(":feature:mission"))
}
