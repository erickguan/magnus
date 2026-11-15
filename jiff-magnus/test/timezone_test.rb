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
    assert_includes zone.inspect, "#<JiffTimezone "
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

  def test_auto_time_registration_can_be_disabled
    [nil, "1", "true", "0", "false", "FALSE"].each do |setting|
      enabled = !["0", "false", "FALSE"].include?(setting)
      code = <<~RUBY
        require "jiff_magnus"
        abort "Unexpected automatic lookup" unless Time.respond_to?(:find_timezone, true) == #{enabled}
        zone = JiffTimezone.find_timezone("Asia/Tokyo")
        abort "Explicit timezone failed" unless Time.at(0, in: zone).utc_offset == 32400
        ENV["JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION"] = "1"
        require "jiff_magnus"
        abort "Repeated require changed lookup" unless Time.respond_to?(:find_timezone, true) == #{enabled}
        ENV["JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION"] = #{setting.inspect}
        JiffTimezone.register
        abort "Explicit registration failed" unless Time.at(0, in: "Asia/Tokyo").utc_offset == 32400
      RUBY
      output, status = Open3.capture2e(
        {"JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION" => setting},
        RbConfig.ruby, "-e", code
      )
      assert status.success?, "setting=#{setting.inspect}: #{output}"
    end
  end

  def test_auto_registration_optout_leaves_existing_lookup_untouched
    ["0", "false"].each do |setting|
      code = <<~RUBY
        def Time.find_timezone(name)
          [:existing, name]
        end
        original = Time.method(:find_timezone)
        require "jiff_magnus"
        abort "Existing lookup changed" unless Time.method(:find_timezone) == original
        abort "Existing lookup failed" unless Time.find_timezone("UTC") == [:existing, "UTC"]
        abort "Constructors unavailable" unless JiffTimezone.utc.iana_name == "UTC"
        JiffTimezone.register
        abort "Explicit replacement failed" unless Time.find_timezone("UTC").iana_name == "UTC"
      RUBY
      output, status = Open3.capture2e(
        {"JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION" => setting},
        RbConfig.ruby, "-e", code
      )
      assert status.success?, "setting=#{setting.inspect}: #{output}"
    end
  end

  def test_registration_explicitly_replaces_an_existing_lookup
    code = <<~RUBY
      def Time.find_timezone(name)
        [:foreign, name]
      end
      Time.singleton_class.send(:private, :find_timezone)
      require "jiff_magnus"
      abort "Existing lookup was replaced" unless Time.send(:find_timezone, "UTC") == [:foreign, "UTC"]
      abort "Constructors are unavailable" unless JiffTimezone.utc.iana_name == "UTC"
      abort "Private lookup changed visibility" if Time.respond_to?(:find_timezone)

      JiffTimezone.register
      abort "Registration did not install public lookup" unless Time.respond_to?(:find_timezone)
      abort "Registered lookup failed" unless Time.at(0, in: "Asia/Tokyo").utc_offset == 32400

      def Time.find_timezone(name)
        :replacement
      end
      JiffTimezone.register
      JiffTimezone.register
      abort "Repeated registration failed" unless Time.find_timezone("UTC").iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_failed_registration_restores_the_existing_lookup
    [
      "def Time.find_timezone(name); :original; end; Time.singleton_class.send(:private, :find_timezone)",
      "def Object.find_timezone(name); :original; end"
    ].each do |setup|
      code = <<~RUBY
        #{setup}
        require "jiff_magnus"
        original = Time.method(:find_timezone)
        was_private = Time.singleton_class.private_method_defined?(:find_timezone, false)
        was_local = Time.singleton_class.method_defined?(:find_timezone, false) || was_private
        def Time.singleton_method_added(name)
          raise "registration hook failed" if name == :find_timezone
        end
        begin
          JiffTimezone.register
          abort "Expected registration to fail"
        rescue RuntimeError => error
          abort error.full_message unless error.message == "registration hook failed"
        end
        class << Time
          remove_method :singleton_method_added
        end
        abort "Original lookup was lost" unless Time.method(:find_timezone) == original
        abort "Visibility changed" unless Time.singleton_class.private_method_defined?(:find_timezone, false) == was_private
        is_local = Time.singleton_class.method_defined?(:find_timezone, false) ||
          Time.singleton_class.private_method_defined?(:find_timezone, false)
        abort "Method ownership changed" unless is_local == was_local

        JiffTimezone.register
        abort "Retry failed" unless Time.find_timezone("UTC").iana_name == "UTC"
        original = Time.method(:find_timezone)
        Time.freeze
        begin
          JiffTimezone.register
          abort "Expected frozen Time to reject registration"
        rescue FrozenError
        end
        abort "Frozen lookup changed" unless Time.method(:find_timezone) == original
      RUBY
      output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
      assert status.success?, output
    end
  end

  def test_reentrant_registration_is_rejected_and_can_retry
    code = <<~RUBY
      require "jiff_magnus"
      original = Time.method(:find_timezone)
      def Time.singleton_method_added(name)
        return unless name == :find_timezone && !@reentered
        @reentered = true
        JiffTimezone.register
      end
      begin
        JiffTimezone.register
        abort "Expected reentrant registration to fail"
      rescue LoadError => error
        abort error.full_message unless error.message.include?("already in progress")
      end
      class << Time
        remove_method :singleton_method_added
      end
      abort "Original lookup was lost" unless Time.method(:find_timezone) == original
      JiffTimezone.register
      abort "Retry failed" unless Time.find_timezone("UTC").iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_initialization_preserves_lookup_added_by_class_hook
    skip "Ruby has no const_added hook" unless Object.respond_to?(:const_added, true)

    code = <<~RUBY
      def Object.const_added(name)
        return unless name == :JiffTimezone
        def Time.find_timezone(name)
          [:application, name]
        end
      end
      require "jiff_magnus"
      abort "Application lookup was replaced" unless Time.find_timezone("UTC") == [:application, "UTC"]
      abort "Constructors are unavailable" unless JiffTimezone.utc.iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_failed_initialization_preserves_replacement_class
    skip "Ruby has no const_added hook" unless Object.respond_to?(:const_added, true)

    code = <<~RUBY
      ApplicationTimezone = Class.new
      def Object.const_added(name)
        return unless name == :JiffTimezone && !@replaced
        @replaced = true
        remove_const(:JiffTimezone)
        const_set(:JiffTimezone, ApplicationTimezone)
        raise "class hook failed"
      end
      begin
        require "jiff_magnus"
        abort "Expected initialization to fail"
      rescue RuntimeError => error
        abort error.full_message unless error.message == "class hook failed"
      end
      class << Object
        remove_method :const_added
      end
      abort "Replacement class was lost" unless JiffTimezone.equal?(ApplicationTimezone)
      abort "Unexpected lookup" if Time.respond_to?(:find_timezone, true)
      Object.send(:remove_const, :JiffTimezone)
      require "jiff_magnus"
      abort "Retry failed" unless JiffTimezone.utc.iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_failed_initialization_preserves_lookup_added_by_class_hook
    skip "Ruby has no const_added hook" unless Object.respond_to?(:const_added, true)

    code = <<~RUBY
      def Object.const_added(name)
        return unless name == :JiffTimezone
        def Time.find_timezone(name)
          [:application, name]
        end
        raise "class hook failed"
      end
      begin
        require "jiff_magnus"
        abort "Expected initialization to fail"
      rescue RuntimeError => error
        abort error.full_message unless error.message == "class hook failed"
      end
      class << Object
        remove_method :const_added
      end
      abort "Failed class was published" if Object.const_defined?(:JiffTimezone, false)
      abort "Application lookup was lost" unless Time.find_timezone("UTC") == [:application, "UTC"]
      require "jiff_magnus"
      abort "Retry replaced application lookup" unless Time.find_timezone("UTC") == [:application, "UTC"]
      JiffTimezone.register
      abort "Explicit registration failed" unless Time.find_timezone("UTC").iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_failed_initialization_keeps_inherited_lookup_disabled
    code = <<~RUBY
      def Object.find_timezone(name)
        :inherited
      end
      Time.singleton_class.send(:undef_method, :find_timezone)
      def Time.singleton_method_added(name)
        raise "resolver hook failed" if name == :find_timezone
      end
      begin
        require "jiff_magnus"
        abort "Expected initialization to fail"
      rescue RuntimeError => error
        abort error.full_message unless error.message == "resolver hook failed"
      end
      class << Time
        remove_method :singleton_method_added
      end
      abort "Disabled inherited lookup became visible" if Time.respond_to?(:find_timezone, true)
      abort "Failed class was published" if Object.const_defined?(:JiffTimezone, false)
      require "jiff_magnus"
      abort "Retry failed" unless Time.find_timezone("UTC").iana_name == "UTC"
    RUBY
    output, status = Open3.capture2e(RbConfig.ruby, "-e", code)
    assert status.success?, output
  end

  def test_non_iana_marshal_limits
    [JiffTimezone.unknown, JiffTimezone.posix("EST5EDT,M3.2.0,M11.1.0")].each do |zone|
      assert_raises(TypeError) { Marshal.dump(Time.at(0, in: zone)) }
    end
  end
end
