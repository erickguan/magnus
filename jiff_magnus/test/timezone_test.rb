# frozen_string_literal: true

require "minitest/autorun"
require "open3"
require "rbconfig"
require "jiff_magnus"

class TimezoneTest < Minitest::Test
  def test_require_initializes_once
    assert_kind_of Class, JiffTimezone
    assert_nil JiffTimezone.superclass.name
    assert_equal JiffTimezone.superclass, JiffTimezone.instance_method(:utc_to_local).owner
    assert_respond_to Time, :find_timezone
    refute require("jiff_magnus")
    assert_equal "Asia/Tokyo", Time.find_timezone("Asia/Tokyo").name
  end

  def test_lookup_returns_frozen_objects
    zone = JiffTimezone.find_timezone("America/New_York")
    assert_instance_of JiffTimezone, zone
    assert_predicate zone, :frozen?
    assert_equal "America/New_York", zone.iana_name
    assert_equal "America/New_York", zone.to_s
    assert_includes zone.inspect, '#<JiffTimezone '
    assert_includes zone.inspect, 'representation="America/New_York"'
    assert_nil JiffTimezone.find_timezone("Not/A_Zone")
    assert_nil Time.find_timezone("Not/A_Zone")
    assert_raises(TypeError) { JiffTimezone.new }
    assert_raises(TypeError) { JiffTimezone.allocate }
  end

  def test_named_zone_preserves_nanoseconds_and_observes_dst
    winter = Time.at(1_704_941_204, 123_456_789, :nsec, in: "America/New_York")
    summer = Time.at(1_720_493_204, 123_456_789, :nsec, in: "America/New_York")
    assert_equal 123_456_789, summer.nsec
    assert_equal(-18_000, winter.utc_offset)
    assert_equal(-14_400, summer.utc_offset)
    refute_predicate winter, :dst?
    assert_predicate summer, :dst?
    assert_equal "EST", winter.strftime("%Z")
    assert_equal "EDT", summer.strftime("%Z")
  end

  def test_local_construction_preserves_fractional_seconds
    fraction = Rational(123_456_789, 1_000_000_000)
    ["America/New_York", JiffTimezone.fixed(19_800),
     JiffTimezone.posix("EST5EDT,M3.2.0,M11.1.0")].each do |zone|
      time = Time.new(2024, 7, 1, 12, 0, fraction, in: zone)
      assert_equal 123_456_789, time.nsec
      assert_equal fraction, time.subsec
      assert_equal time.to_r, time.getutc.to_r
    end
  end

  def test_arithmetic_uses_transition_rules
    before = Time.at(Time.utc(2024, 3, 10, 6, 30).to_i, in: "America/New_York")
    after = before + 3_600
    assert_equal 1, before.hour
    assert_equal 3, after.hour
    assert_same before.zone, after.zone
    assert_equal(-14_400, after.utc_offset)
  end

  def test_local_construction_rejects_gaps_and_folds
    assert_raises(ArgumentError) { Time.new(2024, 3, 10, 2, 30, 0, in: "America/New_York") }
    assert_raises(ArgumentError) { Time.new(2024, 11, 3, 1, 30, 0, in: "America/New_York") }
    early = Time.at(Time.utc(2024, 11, 3, 5, 30).to_i, in: "America/New_York")
    late = Time.at(Time.utc(2024, 11, 3, 6, 30).to_i, in: "America/New_York")
    assert_equal [1, 1], [early.hour, late.hour]
    assert_equal [-14_400, -18_000], [early.utc_offset, late.utc_offset]
  end

  def test_fixed_utc_posix_and_unknown_constructors
    fixed = Time.at(0, in: JiffTimezone.fixed(19_800))
    assert_equal 19_800, fixed.utc_offset
    assert_equal 0, Time.at(0, in: JiffTimezone.utc).utc_offset
    unknown = Time.at(0, in: JiffTimezone.unknown)
    assert_equal "Etc/Unknown", unknown.zone.to_s
    assert_equal 0, unknown.utc_offset
    zone = JiffTimezone.posix("EST5EDT,M3.2.0,M11.1.0")
    assert_equal(-18_000, Time.new(2024, 1, 1, in: zone).utc_offset)
    assert_equal(-14_400, Time.new(2024, 7, 1, in: zone).utc_offset)
    assert_raises(ArgumentError) { JiffTimezone.fixed(100_000) }
    assert_raises(ArgumentError) { JiffTimezone.posix("not a POSIX rule") }
  end

  def test_marshal_restores_named_and_native_fixed_zones
    named = Time.at(1_720_493_204, 123_456_789, :nsec, in: "America/New_York")
    restored = Marshal.load(Marshal.dump(named))
    assert_equal named, restored
    assert_equal named.nsec, restored.nsec
    assert_instance_of JiffTimezone, restored.zone
    assert_equal "America/New_York", restored.zone.name
    fixed = Time.at(0, in: "+05:30")
    assert_equal fixed.utc_offset, Marshal.load(Marshal.dump(fixed)).utc_offset
    code = <<~RUBY
      require "jiff_magnus"
      time = Marshal.load(STDIN.read)
      puts [time.zone.name, time.nsec, time.utc_offset].join(",")
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code, stdin_data: Marshal.dump(named))
    assert status.success?, output
    assert_equal "America/New_York,123456789,-14400", output.strip
  end

  def test_non_iana_marshal_limits
    [JiffTimezone.unknown, JiffTimezone.posix("EST5EDT,M3.2.0,M11.1.0")].each do |zone|
      assert_raises(TypeError) { Marshal.dump(Time.at(0, in: zone)) }
    end
  end
end
