//! JVM: Maven and Gradle resolve from the operator's caches, offline, through
//! the read-only seeds the stage cache names; their own writes stay in the
//! stage cache. The seed is a `group:artifact:version` the operator's caches
//! already hold, named by an environment variable.

use super::{home, listing, seed, Rig};

const APP: &str = "public class App {\n    public static void main(String[] args) {}\n}\n";

fn coordinates(coordinate: &str) -> [&str; 3] {
    let parts: Vec<&str> = coordinate.split(':').collect();
    parts
        .try_into()
        .unwrap_or_else(|_| panic!("{coordinate:?} is not group:artifact:version"))
}

/// `mvn -o -B compile` succeeds on a project depending on the seed, and
/// nothing is written under the host `~/.m2`.
#[test]
#[serial_test::serial]
fn maven_resolves_from_the_host_seed_offline() {
    let test = "maven_resolves_from_the_host_seed_offline";
    let Some(coordinate) = seed(test, "LOOM_TEST_MAVEN_SEED") else {
        return;
    };
    let Some(rig) = Rig::start(test, &["mvn"]) else {
        return;
    };
    let [group, artifact, version] = coordinates(&coordinate);
    rig.write(
        "pom.xml",
        &format!(
            "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
             <modelVersion>4.0.0</modelVersion>\n\
             <groupId>example</groupId><artifactId>probe</artifactId><version>1</version>\n\
             <properties><maven.compiler.source>11</maven.compiler.source>\
             <maven.compiler.target>11</maven.compiler.target>\
             <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding></properties>\n\
             <dependencies><dependency><groupId>{group}</groupId>\
             <artifactId>{artifact}</artifactId><version>{version}</version></dependency>\
             </dependencies>\n</project>\n"
        ),
    );
    rig.write("src/main/java/App.java", APP);
    let host = home().join(".m2");
    let before = listing(&host);
    let result = rig.run("mvn -o -B compile");
    assert!(result.success, "maven did not resolve offline: {result:?}");
    assert!(result.stdout.contains("BUILD SUCCESS"), "{result:?}");
    assert_eq!(before, listing(&host), "the host ~/.m2 changed");
}

/// `gradle --offline --no-daemon compileJava` succeeds on a project depending
/// on the seed, resolving it from the host's read-only dependency cache.
#[test]
#[serial_test::serial]
fn gradle_resolves_from_the_read_only_cache_offline() {
    let test = "gradle_resolves_from_the_read_only_cache_offline";
    let Some(coordinate) = seed(test, "LOOM_TEST_GRADLE_SEED") else {
        return;
    };
    let Some(rig) = Rig::start(test, &["gradle"]) else {
        return;
    };
    rig.write("settings.gradle", "rootProject.name = 'probe'\n");
    rig.write(
        "build.gradle",
        &format!(
            "plugins {{ id 'java' }}\nrepositories {{ mavenCentral() }}\n\
             dependencies {{ implementation '{coordinate}' }}\n"
        ),
    );
    rig.write("src/main/java/App.java", APP);
    let result = rig.run("gradle --offline --no-daemon compileJava");
    assert!(result.success, "gradle did not resolve offline: {result:?}");
    assert!(result.stdout.contains("BUILD SUCCESSFUL"), "{result:?}");
}
