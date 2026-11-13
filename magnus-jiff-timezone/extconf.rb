# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

create_rust_makefile("magnus_jiff_timezone/magnus_jiff_timezone") do |config|
  config.extra_cargo_args << "--crate-type=cdylib"
end
