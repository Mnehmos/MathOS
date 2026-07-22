namespace MathOS.PilotA

def Prime (n : Nat) : Prop :=
  n >= 2 ∧ ∀ d : Fin (n + 1), d.val >= 2 -> d.val < n -> n % d.val ≠ 0

def Odd (n : Nat) : Prop :=
  n % 2 = 1

theorem two_is_prime_and_not_odd : Prime 2 ∧ ¬ Odd 2 := by
  unfold Prime Odd
  decide

theorem every_prime_is_odd_refuted :
    Not (∀ n : Nat, MathOS.PilotA.Prime n -> MathOS.PilotA.Odd n) := by
  intro universal_claim
  exact two_is_prime_and_not_odd.2 (universal_claim 2 two_is_prime_and_not_odd.1)

theorem every_prime_other_than_two_is_odd :
    ∀ n : Nat, MathOS.PilotA.Prime n -> n ≠ 2 -> MathOS.PilotA.Odd n := by
  sorry

end MathOS.PilotA
