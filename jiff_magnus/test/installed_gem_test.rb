# frozen_string_literal: true

require "fileutils"
require "open3"
require "rbconfig"
require "rubygems/package"
require "tmpdir"

package = File.expand_path(ARGV.fetch(0))
checks = File.expand_path("timezone_test.rb", __dir__)
ruby = RbConfig.ruby

Dir.mktmpdir("magnus-timezone-install-") do |directory|
  # Install from outside the checkout. Every native source and manifest needed
  # by this build must come from the gem archive, not from its parent workspace.
  source_gem = File.join(directory, File.basename(package))
  FileUtils.cp(package, source_gem)
  gem_home = File.join(directory, "gems")
  env = {
    "GEM_HOME" => gem_home,
    "GEM_PATH" => ([gem_home] + Gem.path).uniq.join(File::PATH_SEPARATOR),
    "BUNDLE_GEMFILE" => nil,
    "RUBYLIB" => nil,
    "RUBYOPT" => nil,
    "RUSTFLAGS" => nil,
    "CARGO_ENCODED_RUSTFLAGS" => nil
  }
  # GEM_HOME selects the destination. An explicit --install-dir makes RubyGems
  # ignore the already-installed dependencies that GEM_PATH exposes.
  command = [ruby, "-S", "gem", "install", "--local", "--no-document", source_gem]
  puts "Installing #{File.basename(package)} outside the checkout"
  unless system(env, *command, chdir: directory)
    abort "Standalone gem installation failed"
  end

  installed = File.join(gem_home, "gems", "jiff-magnus-#{Gem::Package.new(source_gem).spec.version}")
  abort "Installed gem contains no native library" if Dir[File.join(installed, "lib", "**", "*.#{RbConfig::CONFIG.fetch('DLEXT')}")].empty?
  unless system(env, ruby, checks, chdir: directory)
    abort "Installed gem tests failed"
  end

  # Initialization conflicts must fail in fresh Ruby processes without replacing
  # the pre-existing class or resolver.
  conflicts = [
    ["class JiffTimezone; end; original = JiffTimezone",
     "JiffTimezone.equal?(original) && !Time.respond_to?(:find_timezone, true)"],
    ["def Time.find_timezone(name); :foreign; end",
     "Time.find_timezone('UTC') == :foreign && !Object.const_defined?(:JiffTimezone, false)"]
  ]
  conflicts.each do |setup, preserved|
    code = <<~RUBY
      #{setup}
      begin
        require "jiff_magnus"
        abort "Expected LoadError"
      rescue LoadError => error
        abort error.full_message unless #{preserved}
        abort error.full_message unless error.message.include?("already defined")
      end
    RUBY
    output, status = Open3.capture2e(env, ruby, "-e", code, chdir: directory)
    abort output unless status.success?
  end
  puts "Standalone require, Ruby behavior, and initialization-conflict checks passed"
end
