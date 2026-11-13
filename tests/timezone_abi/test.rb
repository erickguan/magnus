# frozen_string_literal: true

SOURCE_LIBRARY, PEER_LIBRARY, C_LIBRARY = ARGV.shift(3)
require SOURCE_LIBRARY
require PEER_LIBRARY
require C_LIBRARY
require "json"
require "minitest/autorun"
require "open3"
require "rbconfig"
require "tmpdir"
require "fileutils"

class InteroperabilityTest < Minitest::Test
  MARKER = "magnus:timezone:unrepresentable"

  def test_extensions_have_distinct_native_descriptors_and_jiff_versions
    refute_equal TimezoneSource.descriptor_address, TimezonePeer.descriptor_address
    assert_equal "0.2.42", TimezoneSource.jiff_version
    assert_equal "0.2.35", TimezonePeer.jiff_version
    time = Time.at(0, in: MagnusTimezone.find_timezone("America/New_York"))
    assert TimezoneSource.owns(time)
    refute TimezonePeer.owns(time)
  end

  def test_named_zone_round_trips_in_both_directions
    time = Time.at(1_720_440_000, 123_456_789, :nsec, in: "America/New_York")
    peer = TimezonePeer.identity(time)
    assert TimezonePeer.owns(peer)
    refute TimezoneSource.owns(peer)
    [peer, TimezoneSource.identity(peer)].each do |result|
      assert_equal time.to_r, result.to_r
      assert_equal time.utc_offset, result.utc_offset
      assert_equal "America/New_York", result.zone.iana_name
      assert_equal time.dst?, result.dst?
    end
  end

  def test_overlap_instants_and_negative_nanoseconds_cross_extensions
    [-1, Time.utc(2024, 11, 3, 5, 30).to_i, Time.utc(2024, 11, 3, 6, 30).to_i].each do |seconds|
      original = Time.at(seconds, 123_456_789, :nsec, in: "America/New_York")
      peer = TimezonePeer.identity(original)
      restored = TimezoneSource.identity(peer)
      [peer, restored].each do |result|
        assert_equal original.to_r, result.to_r
        assert_equal original.utc_offset, result.utc_offset
        assert_equal original.dst?, result.dst?
      end
    end
  end

  def test_native_and_non_iana_forms_cross_extensions
    [MagnusTimezone.utc, MagnusTimezone.fixed(19_800), MagnusTimezone.unknown,
     MagnusTimezone.posix("EST5EDT,M3.2.0,M11.1.0")].each do |zone|
      time = Time.at(1_720_440_000, 123_456_789, :nsec, in: zone)
      result = TimezonePeer.identity(time)
      assert_equal time.to_r, result.to_r
      assert_equal time.utc_offset, result.utc_offset
      assert_equal time.dst?, result.dst?
    end
    assert_equal "Etc/Unknown", TimezonePeer.identity(TimezoneSource.make("unknown")).zone.to_s
    posix = TimezonePeer.identity(TimezoneSource.make("posix"))
    winter = Time.at(Time.utc(2024, 1, 1).to_i, in: posix.zone)
    assert_equal(-18_000, winter.utc_offset)
  end

  def test_c_header_matches_actual_rust_allocations
    assert_equal "America/New_York", TimezoneC.read(MagnusTimezone.find_timezone("America/New_York"))
    assert_equal "Etc/Unknown", TimezoneC.read(MagnusTimezone.unknown)
    assert_equal "+05:30", TimezoneC.read(MagnusTimezone.fixed(19_800))
    assert_equal 0, TimezoneC.layout[1]
    assert_operator TimezoneC.layout[2], :>=, 1
    refute_respond_to MagnusTimezone.unknown, :abi_version
    refute_respond_to MagnusTimezone.unknown, :jiff_timezone
  end

  def test_rust_reader_accepts_c_typed_data_without_rust_fields
    delegate = MagnusTimezone.find_timezone("America/New_York")
    zone = TimezoneC.wrap(delegate, "America/New_York", 1)
    original = Time.at(1_720_440_000, 987_654_321, :nsec, in: zone)
    [TimezoneSource, TimezonePeer].each do |extension|
      result = extension.identity(original)
      assert_equal original.to_r, result.to_r
      assert_equal original.utc_offset, result.utc_offset
      assert_equal "America/New_York", result.zone.iana_name
    end
  end

  def test_header_and_string_survive_gc_compaction
    zone = MagnusTimezone.find_timezone("America/New_York")
    time = Time.at(0, in: zone)
    3.times do
      GC.start
      GC.verify_compaction_references(double_heap: true, toward: :empty) if GC.respond_to?(:verify_compaction_references)
      assert_equal "America/New_York", TimezoneC.read(zone)
      assert_equal time.to_r, TimezonePeer.identity(time).to_r
    end
  end

  def with_reported_zone(zone)
    time = Time.at(0)
    time.define_singleton_method(:zone) { zone }
    time
  end

  def test_unknown_version_is_rejected_before_reading_more_than_one_byte
    error = assert_raises(TypeError) { TimezonePeer.identity(with_reported_zone(TimezoneC.short_version)) }
    assert_includes error.message, "ABI version"
  end

  def test_old_descriptor_does_not_claim_new_header_layout
    zone = TimezoneC.legacy(MagnusTimezone.unknown)
    error = assert_raises(TypeError) { TimezonePeer.identity(with_reported_zone(zone)) }
    assert_includes error.message, "cannot be represented"
  end

  def test_invalid_portable_representations_report_errors
    [[MARKER, "cannot be transferred"], [nil, "must not be NULL"],
     ["\xFF".b, "not UTF-8"], ["Not/A_Zone", "could not reconstruct"]].each do |text, message|
      zone = TimezoneC.wrap(MagnusTimezone.unknown, text, 1)
      error = assert_raises(TypeError) { TimezonePeer.identity(with_reported_zone(zone)) }
      assert_includes error.message, message
    end
  end

  def test_foreign_ruby_objects_are_not_accepted_by_name
    zone = Object.new
    zone.define_singleton_method(:name) { "America/New_York" }
    zone.define_singleton_method(:iana_name) { "America/New_York" }
    assert_raises(TypeError) { TimezonePeer.identity(with_reported_zone(zone)) }
  end

  def test_custom_named_rules_use_receivers_database
    original = TimezoneSource.make("custom")
    assert_equal 32_400, original.utc_offset
    assert_equal "America/New_York", TimezoneC.read(original.zone)
    assert_equal 32_400, TimezoneSource.identity(original).utc_offset
    reconstructed = TimezonePeer.identity(original)
    assert_equal original.to_r, reconstructed.to_r
    assert_equal(-14_400, reconstructed.utc_offset)
  end

  def test_bundled_database_string_coverage
    entries = TimezoneSource.coverage
    supported = entries.count { |_, printable| printable }
    failures = []
    entries.each do |name, printable|
      next unless printable
      original = TimezoneSource.make(name)
      reconstructed = TimezonePeer.identity(original)
      failures << name unless original.to_r == reconstructed.to_r && original.utc_offset == reconstructed.utc_offset
    rescue TypeError => error
      failures << [name, error.message]
    end
    puts "TIMEZONE_COVERAGE #{JSON.generate(dataset: 'jiff-tzdb 0.1.9 names; July 2024 instant', source_jiff: TimezoneSource.jiff_version, reader_jiff: TimezonePeer.jiff_version, total: entries.length, printable: supported, unrepresentable: entries.length - supported, failed_round_trips: failures.length)}"
    assert_operator entries.length, :>, 500
    assert_equal entries.length, supported
    assert_empty failures
  end

  def test_system_zone_examples_in_isolated_processes
    skip "Unix system-zone fixture" if Gem.win_platform?
    Dir.mktmpdir("magnus-system-zone-") do |directory|
      copied = File.join(directory, "localtime")
      File.binwrite(copied, TimezoneSource.tzif("America/New_York"))
      named = File.join(directory, "zoneinfo", "America", "New_York")
      FileUtils.mkdir_p(File.dirname(named))
      File.symlink(copied, named)
      [[copied, false], [named, true]].each do |path, portable|
        code = <<~RUBY
          require "json"
          ARGV.each { |library| require library }
          time = TimezoneSource.system
          text = TimezoneC.read(time.zone)
          abort "Lost original summer rules" unless time.utc_offset == -14400 && time.dst?
          winter = Time.at(Time.utc(2024, 1, 1).to_i, in: time.zone)
          abort "Lost original winter rules" unless winter.utc_offset == -18000 && !winter.dst?
          abort "Local Rust access lost rules" unless TimezoneSource.identity(time).utc_offset == -14400
          if #{portable}
            abort "Expected named timezone" unless text == "America/New_York"
            abort "Cross-extension transfer failed" unless TimezonePeer.identity(time).to_r == time.to_r
          else
            abort "Expected marker" unless text == #{MARKER.dump}
            begin
              TimezonePeer.identity(time)
              abort "Expected unsupported transfer"
            rescue TypeError => error
              abort error.full_message unless error.message.include?("cannot be transferred")
            end
          end
          puts "SYSTEM_TIMEZONE_COVERAGE " + JSON.generate(jiff: TimezoneSource.jiff_version, tz: ENV.fetch("TZ"), named: #{portable}, portable: #{portable}, ruby_rules_preserved: true)
        RUBY
        output, status = Open3.capture2e({"TZ" => ":#{path}"}, RbConfig.ruby, "-e", code, SOURCE_LIBRARY, PEER_LIBRARY, C_LIBRARY)
        assert status.success?, output
        puts output
      end
    end
  end
end
