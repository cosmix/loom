---
name: loom-model-evaluation
description: Evaluates ML models for performance, fairness, and reliability. Use for metric selection, cross-validation strategies, overfitting/underfitting diagnosis, hyperparameter tuning, LLM evaluation, A/B testing, and production monitoring for model drift.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - model evaluation
  - metrics
  - accuracy
  - precision
  - recall
  - F1
  - F-beta
  - ROC
  - AUC
  - ROC-AUC
  - PR-AUC
  - calibration
  - Brier score
  - confusion matrix
  - cross-validation
  - k-fold
  - stratified
  - data leakage
  - target leakage
  - overfitting
  - underfitting
  - bias-variance tradeoff
  - hyperparameter tuning
  - loss function
  - benchmark
  - model performance
  - classification metrics
  - regression metrics
  - RMSE
  - MAE
  - R2
  - train-test split
  - learning curve
  - validation curve
  - model selection
  - error analysis
  - threshold selection
  - LLM evaluation
  - LLM-as-judge
  - A/B testing
  - champion-challenger
  - model monitoring
  - model drift
  - data drift
  - concept drift
  - PSI
---

# Model Evaluation

## Overview

Choose metrics that match the business cost and data distribution, prevent leakage, validate with the right CV scheme, calibrate and threshold deliberately, and monitor for drift in production. This skill is the decision layer above sklearn/eval tooling.

## Metric selection (the highest-leverage decision)

Wrong metric = confidently shipping a bad model. Pick from the cost structure and class balance, not habit.

| Situation | Use | Avoid / why |
| --- | --- | --- |
| Rare positives (fraud, disease, churn) | **PR-AUC**, F-beta, MCC, recall@fixed-precision | **Accuracy** (a 99%-negative dataset scores 99% by predicting all-negative). **ROC-AUC** looks great even when precision is unusable — it ignores the huge TN base |
| FN much costlier than FP | F-beta with β>1 (recall-weighted), recall@precision floor | plain F1 (β=1 assumes equal cost) |
| FP costlier than FN | precision, F-beta β<1 | recall-optimized metrics |
| Need a probability, not a label | log loss, **Brier score**, calibration curve | thresholded accuracy/F1 |
| Multi-class imbalance | **macro** F1 (equal class weight), MCC | **micro**/weighted (dominated by majority class) |
| Regression with outliers | MAE, median AE, Huber | MSE/RMSE (squares dominated by outliers) |
| Regression, relative error matters | MAPE / SMAPE | RMSE; ⚠ MAPE explodes near zero and is asymmetric (penalizes over-prediction less) |
| Ranking / retrieval | NDCG, MAP, MRR | accuracy |

- **ROC-AUC vs PR-AUC:** ROC-AUC is invariant to class balance (baseline 0.5 always) — misleadingly optimistic when positives are rare. PR-AUC's baseline = positive prevalence, so it exposes the hard problem. Report PR-AUC for imbalanced detection.
- **`average=` trap (sklearn):** `weighted`/`micro` hide minority-class failure; use `macro` (or per-class) when minority classes matter. Default `binary` assumes label `1` is positive.
- **MCC** (Matthews correlation) is the most robust single scalar for imbalanced binary — high only when all four confusion cells are good.
- Always report a **baseline** (majority-class, random-stratified, or last-value for time series). A metric without a baseline is uninterpretable.

## Data leakage (the most common silent invalidator)

Leakage makes offline metrics great and production terrible. Hunt for it explicitly:

| Type | Symptom / cause | Prevention |
| --- | --- | --- |
| **Target leakage** | A feature encodes the label or is only known post-outcome (e.g. `payment_received` predicting `will_pay`) | Ask of every feature: "available at prediction time?" |
| **Preprocessing leakage** | Fit scaler/imputer/encoder/PCA/feature-selection on the **full** dataset before splitting | Fit on train **inside** the CV fold only — use a `Pipeline`, never `fit_transform` on all data |
| **Temporal leakage** | Random shuffle of time series; using future to predict past | Time-ordered split; `TimeSeriesSplit`; features use only past windows |
| **Group leakage** | Same entity (patient/user) rows in both train and test | `GroupKFold` / `StratifiedGroupKFold` on the entity id |
| **Duplicate leakage** | Near-duplicate rows straddle the split | Dedup before splitting |
| **Target encoding leakage** | Mean-target encoding computed across folds | Compute encodings within-fold (out-of-fold) |

⚠ The tell: near-perfect validation scores, or a feature with implausibly high importance. Investigate before celebrating.

## Cross-validation — match the scheme to the data

```python
from sklearn.model_selection import StratifiedKFold, GroupKFold, TimeSeriesSplit
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler

# Preprocessing INSIDE the pipeline → refit per fold, no leakage
pipe = make_pipeline(StandardScaler(), estimator)
cv = StratifiedKFold(n_splits=5, shuffle=True, random_state=42)  # classification default
```

- **Classification:** `StratifiedKFold` (preserves class ratio per fold) — plain `KFold` can leave a rare class absent from a fold.
- **Grouped data:** `StratifiedGroupKFold` — no entity in two folds.
- **Time series:** `TimeSeriesSplit` (expanding/rolling window, forward-chaining) — **never shuffle**; test is always *after* train. Add an embargo/gap between train and test if features use look-back windows.
- **Small data / model selection:** **nested CV** (outer loop = performance estimate, inner loop = hyperparameter search). Tuning and estimating on the same CV inflates the score.
- Report **mean ± std** across folds; a large std means the estimate is unstable (small data / leakage / non-stationarity), not that the mean is trustworthy.
- **Never touch the test set** until final reporting. Repeatedly tuning against the validation set turns it into training data (validation overfitting).

## Calibration (probabilities must mean what they say)

A model can rank well (high AUC) yet be badly calibrated — critical when the score feeds a threshold, expected-value decision, or is shown to users.

- **Diagnose:** reliability curve (`sklearn.calibration.calibration_curve`) + **Brier score** + ECE (expected calibration error). Perfect calibration = predicted 0.7 is correct 70% of the time.
- **Fix:** `CalibratedClassifierCV` — **Platt/sigmoid** (parametric, good for small data or SVM-shaped distortions) or **isotonic** (non-parametric, needs more data, can overfit small sets). Fit calibration on a held-out set, not training data.
- ⚠ Tree ensembles and SVMs are often miscalibrated (over/under-confident); neural nets with modern training tend to be over-confident. Don't assume `predict_proba` is a real probability.

## Threshold selection (separate from training)

The 0.5 default is almost never optimal for imbalanced or asymmetric-cost problems. Choose the threshold **on validation data**, driven by cost:

- Maximize expected utility: `threshold = argmax(TP·v_tp − FP·c_fp − FN·c_fn …)` over the PR/ROC curve.
- Or pick recall@precision-floor (SLA style), or Youden's J on ROC, or F-beta max.
- Tune the threshold on validation, then report the *fixed* threshold's metrics on test. Choosing it on test is leakage.

## Overfitting / underfitting diagnosis

| Signal | Diagnosis | Fix |
| --- | --- | --- |
| High train, low val (large gap) | Overfitting / high variance | Regularization (L1/L2/dropout), more data, early stopping, simpler model, augmentation |
| Low train AND low val | Underfitting / high bias | More capacity, better features, longer training, lower regularization |
| Both curves plateau, small gap | Good fit (or data-limited) | Diminishing returns — more data won't help if curves are flat |

- **Learning curve** (score vs training-set size): gap that closes with more data ⇒ overfitting curable by data; both-low-and-flat ⇒ bias, need a better model.
- **Validation curve** (score vs one hyperparameter): locate the bias/variance sweet spot.
- **Sanity check:** overfit a single tiny batch first — if the model can't reach ~0 loss there, the training loop/loss is broken.

## Hyperparameter tuning

- Grid (small spaces) → **random** (better in high dimensions; most params don't matter) → **Bayesian/Optuna** (sample-efficient) → **Hyperband/ASHA** (early-stop bad trials). Learning rate is usually the highest-impact knob.
- Search log-scale for LR/regularization; select on validation; final metric on the untouched test set.

## LLM / generative evaluation

- **Reference-based** (BLEU/ROUGE/METEOR/exact-match): cheap but weak — penalize valid paraphrases; **BERTScore** is better on semantics. Use for regression testing, not quality ceilings.
- **LLM-as-judge biases** (must mitigate — judges are not neutral):
  - **Position bias:** favors the first (or a fixed side) option → run both orders, average, or count only if consistent.
  - **Verbosity bias:** prefers longer answers → constrain a rubric on correctness, control for length.
  - **Self-preference bias:** a model rates its own family higher → use a different-family judge for cross-model comparisons.
  - **Sycophancy / leniency** and **scale compression** (clusters at 4–5/5) → prefer **pairwise** comparisons over absolute 1–5 scores; use a concrete rubric.
- **Practice:** judge at temperature 0; give an explicit rubric with anchors; require reasoning-before-score (CoT); **anchor to human labels** on a sample and measure judge↔human agreement (Cohen's κ) before trusting it at scale.
- Task-level: pass@k for code, factuality/groundedness for RAG (does the answer cite retrieved context?), safety/toxicity/refusal-rate as guardrails.

## A/B testing for model comparison

- **Peeking is the #1 error:** checking a fixed-horizon test repeatedly and stopping at significance inflates false-positive rate far above α. Fix: pre-compute sample size + fixed horizon, **or** use sequential/always-valid methods (mSPRT, group-sequential, Bayesian) designed for continuous monitoring.
- **Sample Ratio Mismatch (SRM):** observed split ≠ intended (e.g. 48/52 vs 50/50) → chi-square test; a failing SRM invalidates the experiment (assignment/logging bug).
- **Multiple comparisons:** testing many metrics/segments inflates false positives → Bonferroni/BH correction; pre-register the primary metric.
- **Guardrails:** latency, error rate, and fairness must not regress even if the primary metric wins.
- **Novelty/primacy effects** and **seasonality** → run ≥1–2 full business cycles; don't decide on day one.
- Report effect size + CI, not just p<0.05. Statistical significance ≠ practical significance.

## Fairness

- Group metrics: selection rate, TPR, FPR, precision per protected group.
- Definitions (mutually incompatible — pick per context): **demographic parity** (equal selection rate), **equalized odds** (equal TPR & FPR), **equal opportunity** (equal TPR), **calibration within groups**.
- ⚠ You generally **cannot satisfy all** simultaneously (impossibility results) except in degenerate cases — choose based on harm model and document the tradeoff.

## Production monitoring / drift

- **Data drift** (input distribution shifts, label unchanged): per-feature **PSI** (>0.1 moderate, >0.2 significant), KS test, KL divergence. Covariate shift.
- **Concept drift** (P(y|x) changes — the relationship itself): detect via prediction-distribution shift + delayed ground-truth performance tracking; needs retraining, not just re-weighting.
- **Prediction drift:** score distribution moves even before labels arrive — an early warning when labels are delayed.
- **Performance monitoring:** track the live metric vs baseline; alert on threshold; set meaningful thresholds to avoid alert fatigue. Watch **training/serving skew** (different feature code paths).
- Deploy via **shadow mode** → canary/gradual rollout → automated rollback on regression.

## Reference examples

Classification metrics + confusion analysis:

```python
from sklearn.metrics import (classification_report, roc_auc_score,
                             average_precision_score, matthews_corrcoef, confusion_matrix)

report = classification_report(y_true, y_pred, digits=4)            # per-class + macro/weighted
pr_auc = average_precision_score(y_true, y_prob)                    # PR-AUC — use for imbalance
roc_auc = roc_auc_score(y_true, y_prob)
mcc = matthews_corrcoef(y_true, y_pred)                             # robust single scalar
tn, fp, fn, tp = confusion_matrix(y_true, y_pred).ravel()
specificity, sensitivity = tn / (tn + fp), tp / (tp + fn)
```

Leak-free CV with per-fold preprocessing and overfit gap:

```python
from sklearn.model_selection import cross_validate
res = cross_validate(pipe, X, y, cv=cv,
                     scoring=["f1_macro", "average_precision"],
                     return_train_score=True, n_jobs=-1)
gap = res["train_f1_macro"].mean() - res["test_f1_macro"].mean()   # large gap ⇒ overfitting
```

Data drift (KS + PSI):

```python
from scipy.stats import ks_2samp
import numpy as np

def psi(expected, actual, bins=10):
    q = np.quantile(expected, np.linspace(0, 1, bins + 1))
    q[0], q[-1] = -np.inf, np.inf
    e = np.histogram(expected, q)[0] / len(expected) + 1e-6
    a = np.histogram(actual, q)[0] / len(actual) + 1e-6
    return np.sum((a - e) * np.log(a / e))          # >0.2 ⇒ significant shift

ks_stat, p = ks_2samp(baseline_feature, current_feature)  # p < 0.05 ⇒ drift
```

A/B analysis (report effect size + CI, not just p):

```python
from scipy import stats
t, p = stats.ttest_ind(treatment, control)            # fixed-horizon; DON'T peek
lift = (treatment.mean() - control.mean()) / control.mean() * 100
pooled = np.sqrt((control.var(ddof=1) + treatment.var(ddof=1)) / 2)
cohens_d = (treatment.mean() - control.mean()) / pooled
```

## Common pitfalls

1. Accuracy on imbalanced data; ROC-AUC where PR-AUC is needed.
2. Any leakage (preprocessing on full data, temporal shuffle, group straddling, target encoding across folds).
3. Tuning on the test set / peeking at A/B tests / validation overfitting.
4. Reporting point estimates without CIs or a baseline.
5. Trusting `predict_proba` without checking calibration; using the 0.5 threshold blindly.
6. LLM-as-judge without debiasing (position/verbosity/self-preference) or human anchoring.
7. Training/serving skew and un-monitored drift after deployment.

## Checklist — before declaring a model evaluated

- [ ] Metric matches class balance + business cost (PR-AUC/F-beta/MCC for imbalance, not accuracy)
- [ ] Baseline reported (majority/random/last-value)
- [ ] No leakage: preprocessing fit inside folds; temporal/group/duplicate splits correct
- [ ] CV scheme fits the data (stratified / grouped / time-series); mean ± std reported
- [ ] Test set untouched until final; hyperparameters tuned on validation only (nested CV if small)
- [ ] Calibration checked if probabilities are used downstream; threshold chosen on validation by cost
- [ ] Learning/validation curves inspected for over/underfitting
- [ ] Fairness assessed across protected groups with a documented definition
- [ ] (LLM) judge debiased + human-anchored; (A/B) sample size pre-set, SRM checked, no peeking
- [ ] Drift + performance monitoring wired for production

## Resources

Metrics: scikit-learn, HF `evaluate`. Fairness: Fairlearn, AIF360. LLM eval: HELM, `lm-evaluation-harness`, RAGAS. A/B: sequential-testing / experimentation platforms. Monitoring: Evidently, WhyLabs, NannyML.
