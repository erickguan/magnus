# frozen_string_literal: true

require "fileutils"
require "json"
require "open3"
require "rbconfig"
require "tmpdir"

root = File.expand_path("../..", __dir__)
fixtures = File.join(__dir__, "fixtures")

# Build separate Cargo projects and targets so neither Rust code nor TypedData
# descriptors are unified. Fixture sources and lockfiles live in temporary
# directories; each extension has its own ignored Cargo target cache.
Dir.mktmpdir("magnus-timezone-interop-") do |directory|
  libraries = [["TimezoneSource", "0.2.42", true], ["TimezonePeer", "0.2.35", false]].map do |name, version, gem|
    project = File.join(directory, name)
    FileUtils.mkdir_p(File.join(project, "src"))
    FileUtils.cp(File.join(fixtures, "timezone_probe.rs"), File.join(project, "src", "lib.rs"))
    manifest = <<~TOML
      [package]
      name = "timezone-probe"
      version = "0.0.0"
      edition = "2024"
      publish = false
      [workspace]
      [lib]
      name = "timezone_probe"
      crate-type = ["cdylib"]
      [features]
      gem = ["dep:magnus_jiff"]
      [dependencies]
      magnus = { path = #{root.to_json}, default-features = false, features = ["jiff-zoned", "rb-sys"] }
      jiff = { version = "=#{version}", default-features = false, features = ["std", "tz-system", "tzdb-bundle-always"] }
      jiff-tzdb = "=0.1.9"
      rb-sys = { version = "0.9.124", default-features = false, features = ["stable-api-compiled-fallback"] }
      magnus_jiff = { path = #{File.join(root, "magnus_jiff").to_json}, optional = true }
    TOML
    File.write(File.join(project, "Cargo.toml"), manifest)
    command = ["cargo", "build", "--manifest-path", File.join(project, "Cargo.toml"), "--message-format", "json-render-diagnostics"]
    command.concat(["--features", "gem"]) if gem
    env = {
      "MAGNUS_TIMEZONE_TEST_MODULE" => name,
      "MAGNUS_TIMEZONE_TEST_JIFF" => version,
      "RUSTFLAGS" => nil,
      "CARGO_ENCODED_RUSTFLAGS" => nil,
      "CARGO_TARGET_DIR" => File.join(root, "target", "timezone-interop", name)
    }
    puts "Building independent #{name} with Jiff #{version}"
    output, status = Open3.capture2e(env, *command, chdir: project)
    abort output unless status.success?
    artifact = output.lines.filter_map do |line|
      entry = JSON.parse(line) rescue nil
      next unless entry && entry["reason"] == "compiler-artifact" && entry.dig("target", "name") == "timezone_probe"
      entry.fetch("filenames").find { |path| path.end_with?(".so", ".dylib", ".dll") }
    end.last
    abort "No native artifact for #{name}" unless artifact
    library = File.join(project, "timezone_probe.#{RbConfig::CONFIG.fetch('DLEXT')}")
    FileUtils.cp(artifact, library)
    library
  end

  c_project = File.join(directory, "c")
  FileUtils.mkdir_p(c_project)
  FileUtils.cp(File.join(fixtures, "timezone_c.c"), c_project)
  File.write(File.join(c_project, "extconf.rb"), <<~RUBY)
    require "mkmf"
    $INCFLAGS << " -I" + Shellwords.escape(#{File.join(root, "src", "include").dump})
    create_makefile("timezone_c")
  RUBY
  [ [RbConfig.ruby, "extconf.rb"], [RbConfig::CONFIG.fetch("MAKE", "make")] ].each do |command|
    output, status = Open3.capture2e(*command, chdir: c_project)
    abort output unless status.success?
  end
  libraries << File.join(c_project, "timezone_c.#{RbConfig::CONFIG.fetch('DLEXT')}")

  checks = File.join(__dir__, "test.rb")
  abort "Cross-extension tests failed" unless system(RbConfig.ruby, checks, *libraries)
end
