import { FullScreenPanel } from "@/components/common/FullScreenPanel";
import { IconPicker } from "@/components/IconPicker";
import { ProviderIconBox } from "@/components/ProviderIconBox";
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { HoverTip } from "@/components/ui/hover-tip";
import { ImeSafeInput } from "@/components/ui/ime-safe-input";
import { getIconMetadata } from "@/icons/extracted/metadata";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type { ReactNode } from "react";
import { useState } from "react";
import { useWatch, type UseFormReturn } from "react-hook-form";
import { useTranslation } from "react-i18next";
/**
 * 必填标签：红色「*」画在伪元素里（不进标签文字），输入框另带 aria-required 给读屏。
 */
export const REQUIRED_LABEL =
  "after:ms-0.5 after:text-danger-text after:content-['*']";
interface BasicFormFieldsProps {
  form: UseFormReturn<ProviderFormData>;
  /** Slot to render content between icon and name fields */
  beforeNameSlot?: ReactNode;
}
export function BasicFormFields({
  form,
  beforeNameSlot,
}: BasicFormFieldsProps) {
  const { t } = useTranslation();
  const [iconDialogOpen, setIconDialogOpen] = useState(false);
  // useWatch 只让这里重渲染；form.watch 会让整张表单（ProviderForm）跟着每次按键重渲染
  const [currentIcon, currentIconColor, watchedName] = useWatch({
    control: form.control,
    name: ["icon", "iconColor", "name"],
  });
  const providerName = watchedName || "Provider";
  const effectiveIconColor =
    currentIconColor ||
    (currentIcon ? getIconMetadata(currentIcon)?.defaultColor : undefined);
  const handleIconSelect = (icon: string) => {
    const meta = getIconMetadata(icon);
    form.setValue("icon", icon);
    form.setValue("iconColor", meta?.defaultColor ?? "");
    // 单选：点了就是选好了，不用再找「完成」
    setIconDialogOpen(false);
  };
  const iconButton = (
    <>
      <HoverTip
        content={
          currentIcon
            ? t("providerIcon.clickToChange", {
                defaultValue: "点击更换图标",
              })
            : t("providerIcon.clickToSelect", {
                defaultValue: "点击选择图标",
              })
        }
      >
        <button
          type="button"
          onClick={() => setIconDialogOpen(true)}
          aria-label={t("providerIcon.change", { defaultValue: "更换图标" })}
          className="group shrink-0 rounded-[8px]"
        >
          <ProviderIconBox
            icon={currentIcon}
            name={providerName}
            color={effectiveIconColor}
            className="bg-surface transition-colors group-hover:bg-subtle"
          />
        </button>
      </HoverTip>
      <FullScreenPanel
        isOpen={iconDialogOpen}
        title={t("providerIcon.selectIcon", {
          defaultValue: "选择图标",
        })}
        onClose={() => setIconDialogOpen(false)}
      >
        <IconPicker
          value={currentIcon}
          onValueChange={handleIconSelect}
          color={effectiveIconColor}
          defaultName={watchedName ?? ""}
        />
      </FullScreenPanel>
    </>
  );
  return (
    <>
      {/* Slot for additional fields between icon and name */}
      {beforeNameSlot}

      {/* 基础信息 - 网格布局 */}
      <div className="grid grid-cols-2 gap-4">
        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem>
              <FormLabel className={REQUIRED_LABEL}>
                {t("provider.name")}
              </FormLabel>
              <div className="flex items-center gap-2">
                {iconButton}
                <FormControl>
                  <ImeSafeInput
                    ref={field.ref}
                    name={field.name}
                    value={field.value ?? ""}
                    onValueChange={field.onChange}
                    onBlur={field.onBlur}
                    disabled={field.disabled}
                    placeholder={t("provider.namePlaceholder")}
                    aria-required="true"
                  />
                </FormControl>
              </div>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="notes"
          render={({ field }) => (
            <FormItem>
              <FormLabel>{t("provider.notes")}</FormLabel>
              <FormControl>
                <ImeSafeInput
                  ref={field.ref}
                  name={field.name}
                  value={field.value ?? ""}
                  onValueChange={field.onChange}
                  onBlur={field.onBlur}
                  disabled={field.disabled}
                  placeholder={t("provider.notesPlaceholder")}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </div>

      <FormField
        control={form.control}
        name="websiteUrl"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{t("provider.websiteUrl")}</FormLabel>
            <FormControl>
              <ImeSafeInput
                ref={field.ref}
                name={field.name}
                value={field.value ?? ""}
                onValueChange={field.onChange}
                onBlur={field.onBlur}
                disabled={field.disabled}
                placeholder={t("providerForm.websiteUrlPlaceholder")}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  );
}
