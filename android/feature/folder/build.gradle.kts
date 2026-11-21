plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.folder"
}

dependencies {
    implementation(project(":feature:chat"))
    implementation(project(":feature:lesson"))
    implementation(project(":feature:mission"))
    implementation(project(":feature:resources"))
}
