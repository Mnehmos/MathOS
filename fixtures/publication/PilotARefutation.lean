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
    ∀ n : Nat, Prime n -> n ≠ 2 -> Odd n := by
  intro n prime_n not_two
  unfold Prime at prime_n
  unfold Odd
  have two_lt_n : 2 < n :=
    Nat.lt_of_le_of_ne prime_n.1 (fun two_eq_n => not_two two_eq_n.symm)
  have two_lt_succ_n : 2 < n + 1 :=
    Nat.lt_succ_of_le prime_n.1
  have not_even : n % 2 ≠ 0 :=
    prime_n.2 ⟨2, two_lt_succ_n⟩ (Nat.le_refl 2) two_lt_n
  have remainder_lt_two : n % 2 < 2 :=
    Nat.mod_lt n (by decide)
  generalize n % 2 = remainder at not_even remainder_lt_two ⊢
  cases remainder with
  | zero => exact False.elim (not_even rfl)
  | succ remainder =>
      cases remainder with
      | zero => rfl
      | succ remainder =>
          have remainder_lt_zero : remainder < 0 :=
            Nat.lt_of_succ_lt_succ (Nat.lt_of_succ_lt_succ remainder_lt_two)
          exact False.elim (Nat.not_lt_zero remainder remainder_lt_zero)

end MathOS.PilotA
