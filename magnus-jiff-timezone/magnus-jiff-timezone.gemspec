# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "magnus-jiff-timezone"
  manifest = File.read(File.join(__dir__, "Cargo.toml"))
  spec.version = manifest[/^version = "([^"]+)"$/, 1]
  spec.authors = ["Erick Guan"]
  spec.summary = "Jiff timezone rules and bundled timezone data for Ruby Time."
  spec.description = "A native Ruby gem providing MagnusTimezone, IANA name lookup, and Jiff-backed timezone conversions."
  spec.homepage = "https://github.com/matsadler/magnus"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.2"
  spec.require_paths = ["lib"]
  spec.extensions = ["extconf.rb"]
  spec.files = Dir.chdir(__dir__) do
    Dir["lib/**/*.rb", "src/**/*.rs", "extconf.rb", "Cargo.toml",
        "README.md", "LICENSE"].sort + ["include/magnus/timezone.h"]
  end
  spec.metadata["source_code_uri"] = "https://github.com/matsadler/magnus/tree/main/magnus-jiff-timezone"
  spec.add_dependency "rb_sys", "~> 0.9.130"
end
