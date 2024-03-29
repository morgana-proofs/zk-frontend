use std::marker::PhantomData;

use crate::circuit::{Circuit, HasSigtype, Sig, Signals};

use super::poseidon_permutation::PoseidonPermutation;

#[derive(Clone, Copy)]
pub enum SpongeAction {
    Absorb(u32),
    Squeeze(u32),
}

impl SpongeAction {
    pub fn serialize(self) -> u32 {
        match self {
            SpongeAction::Absorb(v) => 1 << 31 ^ v,
            SpongeAction::Squeeze(v) => v,
        }
    }   
}

pub trait TSpongePrivate<C>
where
    C: Circuit + Signals,
    C::Config: HasSigtype<<C as Circuit>::F>,
{
    type DomainSeparator;
    type Field;

    fn rate(&self) -> usize;
    
    fn absorb_pos(&self) -> usize;
    fn set_absorb_pos(&mut self, new_pos: usize);
    fn squeeze_pos(&self) -> usize;
    fn set_squeeze_pos(&mut self, new_pos: usize);

    fn new(c: &mut C, sep: Self::DomainSeparator, rate: usize) -> Self;
    fn add_log(&mut self, action: SpongeAction);
    fn get_log(&self) -> Vec<SpongeAction>;
    fn tag_hasher(&self, items: Vec<u32>) -> Self::Field;
    fn serialized_domain_separator(&self) -> Vec<u32>;
    fn initialize_capacity(&mut self, c: &mut C, capacity: Self::Field);
    fn read_rate_element(&self, offset: usize) -> Sig<C, C::F>;
    fn add_rate_element(&mut self, offset: usize, value: Sig<C, C::F>);
    fn permute(&mut self, c: &mut C);

    fn absorb_one(&mut self, c: &mut C, input: Sig<C, C::F>) {
        if self.absorb_pos() == self.rate() {
            self.permute(c);
            self.set_absorb_pos(0);
        }

        self.add_rate_element(self.absorb_pos(), input);
        
        self.set_absorb_pos(self.absorb_pos() + 1);
        self.set_squeeze_pos(self.rate())
    }

    fn squeeze_one(&mut self, c: &mut C) -> Sig<C, C::F> {
        if self.squeeze_pos() == self.rate() {
            self.permute(c);
            self.set_absorb_pos(0);
            self.set_squeeze_pos(0);
        }

        let ret = self.read_rate_element(self.squeeze_pos()).clone();

        self.set_squeeze_pos(self.squeeze_pos() + 1);
        ret
    }

    fn finalize(&mut self, c: &mut C) {
        let mut preparerd_tag: Vec<u32> = self.get_log().iter().fold(vec![], |mut acc: Vec<SpongeAction>, &n| {
            if let Some(action) = acc.last_mut() {
                match (action, n) {
                    (SpongeAction::Absorb(last), SpongeAction::Absorb(next)) => *last += next,
                    (SpongeAction::Absorb(_), SpongeAction::Squeeze(_)) => acc.push(n),
                    (SpongeAction::Squeeze(_), SpongeAction::Absorb(_)) => acc.push(n),
                    (SpongeAction::Squeeze(last), SpongeAction::Squeeze(next)) => *last += next,
                }
            } else {
                acc.push(n);
            }
            acc
        }).iter().map(|action| {
            action.serialize()
        }).collect();

        preparerd_tag.extend_from_slice(self.serialized_domain_separator().as_slice());

        self.initialize_capacity(c, self.tag_hasher(preparerd_tag))
    }
}

pub trait TSponge<C> : TSpongePrivate<C>
where
    C: Circuit + Signals,
    C::Config: HasSigtype<<C as Circuit>::F>,
{
    fn new(c: &mut C) -> Self;
    fn absorb(&mut self, c: &mut C, inputs: Vec<Sig<C, C::F>>) {
        if inputs.len() == 0 {
            return
        }
        <Self as TSpongePrivate<C>>::add_log(self, SpongeAction::Absorb(inputs.len() as u32));

        for input in inputs {
            <Self as TSpongePrivate<C>>::absorb_one(self, c, input)
        }       
    }

    fn squeeze(&mut self, c: &mut C, length: usize) -> Vec<Sig<C, C::F>> {
        if length == 0 {
            return vec![];
        }
        <Self as TSpongePrivate<C>>::add_log(self, SpongeAction::Squeeze(length as u32));

        (0..length).map(|_| <Self as TSpongePrivate<C>>::squeeze_one(self, c)).collect()
    }

    fn finalize(&mut self, c: &mut C) {
        <Self as TSpongePrivate<C>>::finalize(self, c);
    }
}

pub trait PoseidonImpl<C>
where
    C: Circuit + Signals,
    C::Config: HasSigtype<<C as Circuit>::F>,
{
    type Sponge: TSponge<C>;
}

pub trait Poseidon<ImplInstance: PoseidonImpl<Self>>
where
    Self: Circuit + Signals,
    Self::Config: HasSigtype<<Self as Circuit>::F>,
{
    fn new(&mut self) -> ImplInstance::Sponge {
        <ImplInstance::Sponge as TSponge<Self>>::new(self)
    }

    fn absorb(&mut self, sponge: &mut ImplInstance::Sponge, inputs: Vec<Sig<Self, Self::F>>) {
        TSponge::absorb(sponge, self, inputs)
    }

    fn squeeze(&mut self, sponge: &mut ImplInstance::Sponge, length: usize) -> Vec<Sig<Self, Self::F>> {
        TSponge::squeeze(sponge, self, length)
    }

    fn finalize(&mut self, sponge: &mut ImplInstance::Sponge) {
        TSponge::finalize(sponge, self)
    }
}