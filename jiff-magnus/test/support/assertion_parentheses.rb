# frozen_string_literal: true

require "rubocop/cop/style/method_call_with_args_parentheses"

module RuboCop
  module Cop
    module JiffMagnus
      # Standard extensions use a separate cop name to configure extra rules.
      class AssertionParentheses < Style::MethodCallWithArgsParentheses
      end
    end
  end
end
