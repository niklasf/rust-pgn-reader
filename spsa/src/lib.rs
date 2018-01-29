extern crate rand;

use rand::XorShiftRng;
use rand::distributions::{IndependentSample, Range};

pub struct HyperParameters {
    alpha: f64,
    gamma: f64,
    theta: [f64; 6],
    a_par: f64,
    noise_var: f64,
}

impl Default for HyperParameters {
    fn default() -> HyperParameters {
        HyperParameters {
            alpha: 0.602,
            gamma: 0.101,
            theta: [0.0; 6],
            a_par: 0.3,
            noise_var: 0.2,
        }
    }
}

impl HyperParameters {
    pub fn spsa(&self) -> Spsa {
        assert!(self.noise_var > 0.0);

        Spsa {
            rng: XorShiftRng::new_unseeded(),
            k: 0.0,
            alpha: self.alpha,
            gamma: self.gamma,
            theta: self.theta,
            a_par: self.a_par,
            noise_var: self.noise_var,
        }
    }
}

pub struct Spsa {
    rng: XorShiftRng,
    k: f64,
    alpha: f64,
    gamma: f64,
    theta: [f64; 6],
    a_par: f64,
    noise_var: f64,
}

impl Spsa {
    pub fn step<F>(&mut self, loss: &mut F)
        where F: FnMut([f64; 6]) -> f64
    {
        let _old_theta = self.theta;

        // need tweaking
        let ak = self.a_par / (self.k + 1.0 + 100.0).powf(self.alpha);
        let ck = self.noise_var / (self.k + 1.0).powf(self.gamma);

        let mut ghat = [0.0; 6];

        let ens_size = 2;

        for _ in 0..ens_size {
            let range = Range::new(0, 2);

            let mut delta = [0.0; 6];
            for i in 0..6 {
                delta[i] = f64::from(range.ind_sample(&mut self.rng)) * 2.0 - 1.0;
            }

            let mut theta_plus = self.theta;
            let mut theta_minus = self.theta;
            for i in 0..6 {
                theta_plus[i] += ck * delta[i];
                theta_minus[i] -= ck * delta[i];
            }

            let j_plus = loss(theta_plus);
            let j_minus = loss(theta_minus);

            for i in 0..6 {
                ghat[i] += (j_plus - j_minus) / (2.0 * ck * delta[i]);
            }
        }

        for i in 0..6 {
            self.theta[i] -= ak * ghat[i];
        }

        self.k += 1.0;
    }

    pub fn theta(&self) -> [f64; 6] {
        self.theta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut loss = |x: [f64; 6]| {
            x[0].powi(2) +
            x[1].powi(2) +
            x[2].powi(2) +
            x[3].powi(2) +
            x[4].powi(2) +
            x[5].powi(2)
        };

        let mut spsa = HyperParameters::default().spsa();

        for i in 0..1000 {
            println!("{}: {:?}", i, spsa.theta());
            spsa.step(&mut loss);
        }

        assert!(false);
    }
}
